use std::time::Duration;

use bevy::input::gamepad::{GamepadRumbleIntensity, GamepadRumbleRequest};
use bevy::prelude::*;
use bevy::transform::TransformSystems;

use crate::combat::HitLanded;
use crate::models::{Player, SceneCamera, Session};
use crate::rules::{Stat, Stats};

pub fn plugin(app: &mut App) {
    app.insert_resource(HitStop::default())
        .insert_resource(ScreenShake::default())
        .add_observer(on_hit_stop)
        .add_observer(on_screen_shake)
        .add_observer(on_rumble)
        .add_systems(Update, tick_hit_stop)
        .add_systems(
            PostUpdate,
            apply_camera_shake
                .after(bevy_third_person_camera::CameraSyncSet)
                .before(TransformSystems::Propagate),
        );
}

// ── Hit Stop ────────────────────────────────────────────────────────

#[derive(Resource, Default)]
pub struct HitStop {
    pub remaining: f32,
}

impl HitStop {
    pub const MAX_DURATION: f32 = 0.12;
}

fn on_hit_stop(
    on: On<HitLanded>,
    mut hit_stop: ResMut<HitStop>,
    mut time: ResMut<Time<Virtual>>,
    player: Query<&Stats, With<Player>>,
    local_check: Query<(), With<crate::combat::PlayerCombatant>>,
) {
    let event = on.event();

    if local_check.get(event.source).is_err() {
        return;
    }

    let duration = event.feedback.hit_stop_duration;
    if duration <= 0.0 {
        return;
    }

    let attack_speed = player
        .single()
        .map(|s| s.get(&Stat::AttackSpeed))
        .unwrap_or(1.0);
    let base_reduction = ((attack_speed - 1.0) * 0.5).clamp(0.0, 0.8);
    let speed_reduction = if event.is_crit {
        base_reduction * 0.5
    } else {
        base_reduction
    };

    let adjusted = (duration * (1.0 - speed_reduction)).max(0.01);
    hit_stop.remaining = hit_stop.remaining.max(adjusted).min(HitStop::MAX_DURATION);
    time.set_relative_speed(0.05);
}

fn tick_hit_stop(
    real_time: Res<Time<Real>>,
    mut hit_stop: ResMut<HitStop>,
    mut time: ResMut<Time<Virtual>>,
) {
    if hit_stop.remaining <= 0.0 {
        return;
    }

    hit_stop.remaining -= real_time.delta_secs();

    if hit_stop.remaining <= 0.0 {
        hit_stop.remaining = 0.0;
        time.set_relative_speed(1.0);
    }
}

// ── Screen Shake ────────────────────────────────────────────────────

#[derive(Resource, Default)]
pub struct ScreenShake {
    pub trauma: f32,
}

impl ScreenShake {
    pub const DECAY: f32 = 2.5;
    pub const MAX_TRANSLATION: f32 = 0.25;
    pub const NOISE_SPEED: f32 = 25.0;
    pub const EXPONENT: f32 = 2.0;
}

fn on_screen_shake(
    on: On<HitLanded>,
    mut shake: ResMut<ScreenShake>,
    local_check: Query<(), With<crate::combat::PlayerCombatant>>,
) {
    if local_check.get(on.event().source).is_err() {
        return;
    }
    let intensity = on.event().feedback.shake_intensity;
    let diminish = 1.0 - shake.trauma * 0.7;
    shake.trauma = (shake.trauma + intensity * diminish).min(0.7);
}

fn apply_camera_shake(
    time: Res<Time>,
    session: Res<Session>,
    mut shake: ResMut<ScreenShake>,
    mut camera: Query<&mut Transform, With<SceneCamera>>,
) {
    shake.trauma = (shake.trauma - ScreenShake::DECAY * time.delta_secs()).max(0.0);

    if !session.screen_shake || shake.trauma <= 0.0 {
        return;
    }

    let Ok(mut transform) = camera.single_mut() else {
        return;
    };

    let shake_amount = shake.trauma.powf(ScreenShake::EXPONENT);
    let t = time.elapsed_secs() * ScreenShake::NOISE_SPEED;

    let x_noise = (t * 1.0).sin() * 0.5 + (t * 2.3).cos() * 0.3 + (t * 4.1).sin() * 0.2;
    let y_noise = (t * 1.7).cos() * 0.5 + (t * 3.1).sin() * 0.3 + (t * 5.3).cos() * 0.2;

    transform.translation.x += x_noise * shake_amount * ScreenShake::MAX_TRANSLATION;
    transform.translation.y += y_noise * shake_amount * ScreenShake::MAX_TRANSLATION;
}

// ── Gamepad Rumble ──────────────────────────────────────────────────

fn on_rumble(
    on: On<HitLanded>,
    gamepads: Query<Entity, With<Gamepad>>,
    mut rumble: MessageWriter<GamepadRumbleRequest>,
    local_check: Query<(), With<crate::combat::PlayerCombatant>>,
) {
    if local_check.get(on.event().source).is_err() {
        return;
    }

    let feedback = &on.event().feedback;

    if feedback.rumble_strong <= 0.0 && feedback.rumble_weak <= 0.0 {
        return;
    }

    for gamepad in gamepads.iter() {
        rumble.write(GamepadRumbleRequest::Add {
            gamepad,
            duration: Duration::from_millis(feedback.rumble_duration as u64),
            intensity: GamepadRumbleIntensity {
                strong_motor: feedback.rumble_strong,
                weak_motor: feedback.rumble_weak,
            },
        });
    }
}
