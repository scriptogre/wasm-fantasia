use super::*;

pub const IDLE_TO_RUN_TRESHOLD: f32 = 0.01;

pub use crate::combat::{GroundPoundImpact, LandingImpact};

/// Fired when the player releases a charge jump. Multiple systems react independently
/// (camera shake, VFX, audio, rumble).
#[derive(Event)]
pub struct JumpLaunched {
    pub charge_time: f32,
    pub height: f32,
    pub position: Vec3,
}

/// Tracks whether the player was airborne last frame for landing detection.
#[derive(Component, Default)]
pub struct AirborneTracker {
    pub was_airborne: bool,
    pub peak_downward_velocity: f32,
}

/// Apply response curve to stick input for more precise control at low values.
/// Exponent > 1.0 makes small inputs smaller (more precision), large inputs relatively larger.
fn apply_response_curve(input: Vec2, exponent: f32) -> Vec2 {
    let length = input.length();
    if length < 0.001 {
        return Vec2::ZERO;
    }
    // Preserve direction, apply curve to magnitude
    let curved_length = length.powf(exponent);
    input.normalize() * curved_length
}

/// Movement stick uses steeper curve for precise positioning at low values,
/// requiring deliberate full input to reach top speed — sells mass and weight.
const MOVEMENT_CURVE_EXPONENT: f32 = 1.6;

/// Smoothed input for Prototype-style exponential acceleration and slide-stop.
/// Asymmetric lerp: fast ramp-up, slow decay creates momentum/inertia feel.
#[derive(Resource, Default)]
pub struct SmoothedInput {
    pub current: Vec2,
}

/// Ramp-up lerp speed — responsive but not instant, lets acceleration sell the weight
const INPUT_RAMP_UP_SPEED: f32 = 30.0;
/// Slow-down lerp speed — momentum slide-to-stop, character has to decelerate
const INPUT_SLOW_DOWN_SPEED: f32 = 5.0;

fn jump_action() -> ControlScheme {
    ControlScheme::Jump(TnuaBuiltinJump {
        allow_in_air: false,
        ..Default::default()
    })
}

// ============================================================================
// INPUT BUFFERING
// Queue inputs briefly so they execute when possible (e.g., jump on landing)
// ============================================================================

/// How long buffered inputs remain valid
const BUFFER_DURATION: f32 = 0.12; // 120ms - feels responsive without being sloppy

pub struct BufferedJump {
    pub buffer_timer: f32,
    pub charge_time: f32,
}

#[derive(Resource, Default)]
pub struct InputBuffer {
    pub jump: Option<BufferedJump>,
    pub attack: Option<f32>,
}

impl InputBuffer {
    pub fn buffer_jump(&mut self, charge_time: f32) {
        self.jump = Some(BufferedJump {
            buffer_timer: BUFFER_DURATION,
            charge_time,
        });
    }
    pub fn buffer_attack(&mut self) {
        self.attack = Some(BUFFER_DURATION);
    }
    pub fn consume_jump(&mut self) -> Option<BufferedJump> {
        self.jump.take()
    }
    pub fn consume_attack(&mut self) -> bool {
        self.attack.take().is_some()
    }
    pub fn tick(&mut self, dt: f32) {
        if let Some(ref mut buffered) = self.jump {
            buffered.buffer_timer -= dt;
            if buffered.buffer_timer <= 0.0 {
                self.jump = None;
            }
        }
        if let Some(t) = &mut self.attack {
            *t -= dt;
            if *t <= 0.0 {
                self.attack = None;
            }
        }
    }
}

// ============================================================================
// JUMP
// ============================================================================

/// Jump height in meters.
pub const JUMP_HEIGHT: f32 = 6.0;

/// Tracks jump state so the movement system can feed the jump action every frame.
/// Uses a resource (not a component) to avoid deferred-command timing gaps.
#[derive(Resource, Default)]
pub struct JumpState {
    pub active: bool,
}

/// Marker: player is currently holding sprint.
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct Sprinting;

const ROLL_IMPULSE_SPEED: f32 = 16.0;

/// Player is performing a dodge roll. Maintains velocity via direct physics impulse.
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct RollingState {
    pub timer: Timer,
    pub direction: Vec3,
}

/// Player is diving downward for a ground pound attack.
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct GroundPoundState;

const GROUND_POUND_SPEED: f32 = 40.0;

/// Player is stunned after landing from a fall. Dampens movement.
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct LandingStun {
    pub timer: Timer,
}

/// Fired when the player takes a footstep while grounded. Triggers dust VFX.
#[derive(Event)]
pub struct Footstep {
    pub position: Vec3,
    pub is_sprinting: bool,
}

// ============================================================================

pub fn plugin(app: &mut App) {
    app.init_resource::<InputBuffer>()
        .init_resource::<SmoothedInput>()
        .init_resource::<JumpState>()
        .add_systems(
            Update,
            (
                ramp_sprint_speed,
                movement.in_set(TnuaUserControlsSystems),
                tick_input_buffer,
                detect_landing,
                tick_rolling_state,
                tick_landing_stun,
                tick_ground_pound,
                process_buffered_jump.after(tick_input_buffer),
            )
                .run_if(in_state(Screen::Gameplay)),
        )
        .add_observer(on_jump)
        .add_observer(on_landing_stun)
        .add_observer(sprint_start)
        .add_observer(sprint_end)
        .add_observer(crouch_in)
        .add_observer(crouch_out);
}

fn tick_input_buffer(time: Res<Time>, mut buffer: ResMut<InputBuffer>) {
    buffer.tick(time.delta_secs());
}

/// Tnua configuration is tricky to grasp from the get go, this is the best demo:
/// <https://github.com/idanarye/bevy-tnua/blob/main/demos/src/character_control_systems/platformer_control_systems.rs>
fn movement(
    time: Res<Time>,
    cfg: Res<Config>,
    navigate: Query<&Action<Navigate>>,
    crouch: Query<&Action<Crouch>>,
    camera: Query<&Transform, With<SceneCamera>>,
    mut smoothed_input: ResMut<SmoothedInput>,
    jump_state: Res<JumpState>,
    mut player_query: Query<(
        &mut Player,
        &mut TnuaController<ControlScheme>,
        Option<&RollingState>,
        Has<GroundPoundState>,
        Has<Sprinting>,
    )>,
) -> Result {
    let Ok(navigate) = navigate.single() else {
        for (_player, mut controller, _, _, _) in player_query.iter_mut() {
            controller.basis = TnuaBuiltinWalk {
                desired_motion: Vec3::ZERO,
                desired_forward: None,
            };
        }
        return Ok(());
    };
    let navigate = *navigate;
    let crouch = crouch.single().copied().unwrap_or_default();
    let dt = time.delta_secs();

    for (player, mut controller, rolling, ground_pounding, _is_sprinting) in player_query.iter_mut()
    {
        let cam_transform = camera.single()?;
        let curved_input = apply_response_curve(*navigate, MOVEMENT_CURVE_EXPONENT);

        // Input smoothing: slow decay only while sprinting for momentum slide-to-stop
        let target_input = curved_input;
        let is_ramping_up = target_input.length_squared() > smoothed_input.current.length_squared();
        let lerp_speed = if is_ramping_up {
            INPUT_RAMP_UP_SPEED
        } else if _is_sprinting {
            INPUT_SLOW_DOWN_SPEED
        } else {
            INPUT_RAMP_UP_SPEED // instant stop when not sprinting
        };
        let current = smoothed_input.current;
        smoothed_input.current += (target_input - current) * (lerp_speed * dt).min(1.0);

        // Kill smoothed input below threshold to ensure full stop
        if smoothed_input.current.length_squared() < 0.001 {
            smoothed_input.current = Vec2::ZERO;
        }

        let direction = cam_transform.movement_direction(smoothed_input.current);

        // Speed-dependent turn rate clamping (Prototype: snappy at walk, restricted at sprint)
        let sprint_speed = cfg.player.movement.speed * cfg.player.movement.sprint_factor;
        let actual_speed = controller.basis_memory.running_velocity.length();
        let speed_ratio = (actual_speed / sprint_speed).clamp(0.0, 1.0);
        // radians/sec: ~full circle at walk, ~86°/s (1.5 rad/s) at sprint — wide arcs at speed
        let max_turn_rate = std::f32::consts::TAU * (1.0 - speed_ratio) + 1.5 * speed_ratio;

        // Limit direction change based on turn rate
        let desired_forward =
            if actual_speed > IDLE_TO_RUN_TRESHOLD && direction.length_squared() > 0.01 {
                let current_forward = controller.basis_memory.running_velocity.normalize_or_zero();
                if current_forward.length_squared() > 0.01 {
                    let max_angle = max_turn_rate * dt;
                    let angle = current_forward.xz().angle_to(direction.xz());
                    if angle.abs() > max_angle {
                        let clamped_angle = angle.clamp(-max_angle, max_angle);
                        let rot = Quat::from_rotation_y(clamped_angle);
                        let turned = rot * current_forward;
                        Dir3::new(turned).ok()
                    } else {
                        Dir3::new(direction).ok()
                    }
                } else {
                    Dir3::new(direction).ok()
                }
            } else {
                Dir3::new(direction).ok()
            };

        // During roll or ground pound, suppress Tnua movement so it doesn't fight the impulse
        let desired_motion = if rolling.is_some() || ground_pounding {
            Vec3::ZERO
        } else {
            direction * player.speed
        };

        controller.initiate_action_feeding();
        controller.basis = TnuaBuiltinWalk {
            desired_motion,
            desired_forward,
        };

        // Keep feeding jump action every frame while jump is in progress
        if jump_state.active {
            controller.action(jump_action());
        }

        // Check if crouch is currently active and apply TnuaBuiltinCrouch as an action
        if *crouch && !jump_state.active {
            controller.action(ControlScheme::Crouch(TnuaBuiltinCrouch));
        }
    }

    Ok(())
}

/// Check if player is grounded (for input buffering)
fn is_grounded(controller: &TnuaController<ControlScheme>) -> bool {
    controller.basis_memory.standing_on_entity().is_some()
}

// ── Charge Jump Observers ──────────────────────────────────────────

/// Jump pressed — immediately jump if grounded, otherwise buffer for landing.
fn on_jump(
    on: On<Start<Jump>>,
    mut commands: Commands,
    mut buffer: ResMut<InputBuffer>,
    mut jump_state: ResMut<JumpState>,
    query: Query<(&TnuaController<ControlScheme>, &Transform), With<Player>>,
) {
    let Ok((controller, transform)) = query.get(on.context) else {
        return;
    };

    if !is_grounded(controller) {
        buffer.buffer_jump(0.0);
        return;
    }

    // Just flag it — the movement system will feed the action to Tnua every frame
    jump_state.active = true;
    commands.trigger(JumpLaunched {
        charge_time: 0.0,
        height: JUMP_HEIGHT,
        position: transform.translation,
    });
}

/// Execute buffered jump when landing.
fn process_buffered_jump(
    mut buffer: ResMut<InputBuffer>,
    mut jump_state: ResMut<JumpState>,
    mut commands: Commands,
    player_query: Query<(&Transform, &TnuaController<ControlScheme>), With<Player>>,
) {
    if buffer.jump.is_none() {
        return;
    }

    let Ok((transform, controller)) = player_query.single() else {
        return;
    };

    if !is_grounded(controller) {
        return;
    }

    let Some(_buffered) = buffer.consume_jump() else {
        return;
    };

    jump_state.active = true;
    commands.trigger(JumpLaunched {
        charge_time: 0.0,
        height: JUMP_HEIGHT,
        position: transform.translation,
    });
}

/// Detect when the player transitions from airborne to grounded.
/// Fires LandingImpact with the peak downward velocity tracked during the fall.
/// Uses raw avian3d LinearVelocity (not Tnua's filtered velocity) for accuracy.
fn detect_landing(
    mut commands: Commands,
    mut jump_state: ResMut<JumpState>,
    mut query: Query<
        (
            Entity,
            &TnuaController<ControlScheme>,
            &Transform,
            &LinearVelocity,
            &mut AirborneTracker,
            Has<GroundPoundState>,
        ),
        With<Player>,
    >,
) {
    let Ok((entity, controller, transform, linear_velocity, mut tracker, is_ground_pounding)) =
        query.single_mut()
    else {
        return;
    };

    let grounded = is_grounded(controller);

    if !grounded {
        // Track peak downward velocity while airborne using raw physics velocity
        let vy = linear_velocity.y;
        if vy < tracker.peak_downward_velocity {
            tracker.peak_downward_velocity = vy;
        }
        tracker.was_airborne = true;
    } else if tracker.was_airborne {
        // Ground pound landing — fire AOE impact and remove state
        if is_ground_pounding {
            commands.trigger(GroundPoundImpact {
                position: transform.translation,
            });
            commands.entity(entity).remove::<GroundPoundState>();
        }

        // Normal landing impact — still fires so existing VFX/shake/stun scale with velocity
        let impact_velocity = tracker.peak_downward_velocity.abs();
        if impact_velocity > 3.0 {
            commands.trigger(LandingImpact {
                velocity_y: impact_velocity,
                position: transform.translation,
            });
        }
        tracker.was_airborne = false;
        tracker.peak_downward_velocity = 0.0;
        jump_state.active = false;
    }
}

/// Slam straight down during ground pound — zero horizontal velocity, force downward.
fn tick_ground_pound(
    mut query: Query<&mut LinearVelocity, (With<Player>, With<GroundPoundState>)>,
) {
    for mut velocity in query.iter_mut() {
        velocity.x = 0.0;
        velocity.z = 0.0;
        velocity.y = -GROUND_POUND_SPEED;
    }
}

/// Maintain roll velocity during the roll, ease out in the last 30%, remove when done.
fn tick_rolling_state(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut RollingState, &mut LinearVelocity), With<Player>>,
) {
    for (entity, mut rolling, mut linear_velocity) in query.iter_mut() {
        if rolling.timer.tick(time.delta()).just_finished() {
            commands.entity(entity).remove::<RollingState>();
            continue;
        }

        let t = rolling.timer.fraction();
        // Quadratic ease-out: fast start, smooth deceleration across full duration
        let speed_factor = 1.0 - t * t;

        linear_velocity.0 = rolling.direction * ROLL_IMPULSE_SPEED * speed_factor;
    }
}

/// Insert LandingStun on landing impact, scaled by fall velocity.
fn on_landing_stun(
    on: On<LandingImpact>,
    mut commands: Commands,
    query: Query<Entity, With<Player>>,
) {
    let event = on.event();
    // Scale stun duration: light fall (3 m/s) → 0.25s, heavy fall (25+ m/s) → 0.8s
    let t = ((event.velocity_y - 3.0) / 22.0).clamp(0.0, 1.0);
    let duration = 0.25 + 0.55 * t;

    for entity in query.iter() {
        commands.entity(entity).try_insert(LandingStun {
            timer: Timer::from_seconds(duration, TimerMode::Once),
        });
    }
}

/// Tick landing stun timer and remove when done.
fn tick_landing_stun(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut LandingStun), With<Player>>,
) {
    for (entity, mut stun) in query.iter_mut() {
        if stun.timer.tick(time.delta()).just_finished() {
            commands.entity(entity).remove::<LandingStun>();
        }
    }
}

/// How fast sprint speed ramps up (units/sec² toward target).
/// At 6.0, it takes ~2.5s to go from jog (6.5) to full sprint (22.1).
const SPRINT_RAMP_SPEED: f32 = 6.0;
/// How fast sprint speed ramps back down when releasing sprint.
const SPRINT_DERAMP_SPEED: f32 = 12.0;

fn sprint_start(
    on: On<Start<Sprint>>,
    mut commands: Commands,
) {
    // Don't set player.speed instantly — the ramp_sprint_speed system handles the gradual buildup.
    commands.entity(on.context).try_insert(Sprinting);
}

fn sprint_end(
    on: On<Complete<Sprint>>,
    mut commands: Commands,
) {
    // Speed ramps back down in ramp_sprint_speed system.
    commands.entity(on.context).try_remove::<Sprinting>();
}

/// Gradually ramp player.speed toward sprint or jog target.
/// Creates the Prototype-style sustained acceleration buildup.
fn ramp_sprint_speed(
    time: Res<Time>,
    cfg: Res<Config>,
    mut player_query: Query<(&mut Player, Has<Sprinting>)>,
) {
    let dt = time.delta_secs();
    let base_speed = cfg.player.movement.speed;
    let max_sprint = base_speed * cfg.player.movement.sprint_factor;

    for (mut player, is_sprinting) in player_query.iter_mut() {
        let target = if is_sprinting { max_sprint } else { base_speed };
        let ramp = if is_sprinting { SPRINT_RAMP_SPEED } else { SPRINT_DERAMP_SPEED };

        if (player.speed - target).abs() < 0.01 {
            player.speed = target;
        } else if player.speed < target {
            player.speed = (player.speed + ramp * dt).min(target);
        } else {
            player.speed = (player.speed - ramp * dt).max(target);
        }
    }
}

pub fn crouch_in(
    on: On<Start<Crouch>>,
    cfg: Res<Config>,
    mut player: Query<&mut Player, With<PlayerCtx>>,
    mut tnua: Query<(&mut TnuaAvian3dSensorShape, &mut Collider), With<Player>>,
) -> Result {
    let (mut avian_sensor, mut collider) = tnua.single_mut()?;
    let mut player = player.get_mut(on.context)?;

    collider.set_scale(Vec3::new(1.0, 0.5, 1.0), 4);
    avian_sensor.0.set_scale(Vec3::new(1.0, 0.5, 1.0), 4);
    player.speed *= cfg.player.movement.crouch_factor;

    Ok(())
}

pub fn crouch_out(
    on: On<Complete<Crouch>>,
    cfg: Res<Config>,
    mut player: Query<&mut Player, With<PlayerCtx>>,
    mut tnua: Query<
        (&mut TnuaAvian3dSensorShape, &mut Collider),
        (With<Player>, Without<SceneCamera>),
    >,
) -> Result {
    let (mut avian_sensor, mut collider) = tnua.get_mut(on.context)?;
    let mut player = player.get_mut(on.context)?;

    collider.set_scale(Vec3::ONE, 4);
    avian_sensor.0.set_scale(Vec3::ONE, 4);
    player.speed = cfg.player.movement.speed;

    Ok(())
}
