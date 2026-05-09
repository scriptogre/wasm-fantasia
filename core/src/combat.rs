//! Shared combat logic — constants, hit detection, timing, feedback.

/// Default combat stats — single source of truth for client and server.
pub mod defaults {
    pub const HEALTH: f32 = 100.0;
    pub const ATTACK_DAMAGE: f32 = 25.0;
    pub const CRIT_CHANCE: f32 = 0.20;
    pub const CRIT_MULTIPLIER: f32 = 2.5;
    pub const ATTACK_RANGE: f32 = 3.6;
    pub const ATTACK_ARC: f32 = 150.0;
    /// Max vertical distance between attacker and target for a hit to land.
    pub const ATTACK_VERTICAL_REACH: f32 = 2.0;
    /// Knockback velocity in m/s applied as an impulse. Crits multiply this by CritMultiplier.
    pub const KNOCKBACK: f32 = 6.0;
    pub const ATTACK_SPEED: f32 = 1.0;
    pub const STACK_DECAY: f32 = 2.5;
    pub const ATTACK_COOLDOWN_SECS: f32 = 0.42;
    pub const ENEMY_HEALTH: f32 = 500.0;
    pub const ENEMY_DETECTION_RANGE: f32 = 15.0;
    pub const ENEMY_ATTACK_RANGE: f32 = 2.0;
    pub const ENEMY_WALK_SPEED: f32 = 2.0;
    pub const ENEMY_ATTACK_COOLDOWN: f32 = 2.0;
    pub const ENEMY_ATTACK_DAMAGE: f32 = 10.0;
    /// Attack animation windup before hit (seconds).
    pub const ENEMY_ATTACK_WINDUP: f32 = 0.4;
    /// Attack animation hit frame (seconds).
    pub const ENEMY_ATTACK_HIT: f32 = 0.55;
    /// Full committed attack duration (seconds).
    pub const ENEMY_ATTACK_DURATION: f32 = 1.0;
    /// Hysteresis: must exceed this distance to leave attack idle (> ATTACK_RANGE).
    pub const ENEMY_ATTACK_DISENGAGE: f32 = 2.5;
    /// Minimum planar speed before enemies should visibly play locomotion.
    /// Filters out tiny settle jitter while keeping separation-driven shuffling
    /// on the walk clip instead of idle.
    pub const ENEMY_ANIMATION_SPEED_EPSILON: f32 = 0.1;

    /// Enemies within this radius push each other apart.
    pub const ENEMY_SEPARATION_RADIUS: f32 = 1.2;
    /// Separation speed in m/s (higher than walk speed so separation wins).
    pub const ENEMY_SEPARATION_STRENGTH: f32 = 2.0;
    /// Spawn ring inner radius (meters from player).
    pub const ENEMY_SPAWN_RADIUS_MIN: f32 = 10.0;
    /// Spawn ring outer radius (meters from player).
    pub const ENEMY_SPAWN_RADIUS_MAX: f32 = 25.0;

    /// Mass for enemy physics bodies (kg). Used for impulse-to-velocity
    /// conversion in knockback. Shared between server physics and combat.
    pub const ENEMY_MASS: f32 = 50.0;

    /// Y position for spawned enemies. Must match the physics-settled capsule
    /// center height so enemies stand on the ground, not sink into it.
    /// The server physics capsule(0.5, 1.0) settles at ~Y=1.0 on the floor plane.
    pub const ENEMY_SPAWN_Y: f32 = 1.0;
}

/// Committed AI decision. States with minimum durations cannot be interrupted.
/// Both client (singleplayer) and server (multiplayer) call this to ensure
/// identical behavior logic. The caller handles movement/DB writes.
/// Decision table (after committed Attack expires):
///
/// | Distance           | Cooldown ready | Result |
/// |--------------------|----------------|--------|
/// | ≤ ATTACK_RANGE     | yes            | Attack |
/// | ≤ ATTACK_RANGE     | no             | Idle   |
/// | RANGE..DISENGAGE   | yes            | Chase  |
/// | RANGE..DISENGAGE   | no             | Idle   |
/// | > DISENGAGE        | *              | Chase  |
///
/// The RANGE..DISENGAGE band prevents oscillation: enemies waiting for
/// cooldown stay Idle (no Chase↔Idle flicker from separation forces),
/// but resume Chase once cooldown is ready to close the gap and attack.
pub fn enemy_ai_decision(
    current_state: EnemyBehaviorKind,
    state_elapsed: f32,
    distance: f32,
    attack_cooldown_ready: bool,
) -> EnemyBehaviorKind {
    if current_state == EnemyBehaviorKind::Attack {
        if state_elapsed < defaults::ENEMY_ATTACK_DURATION {
            // Attack committed — cannot be interrupted.
            return EnemyBehaviorKind::Attack;
        }
        // Attack duration expired — MUST transition to Idle so that
        // last_attack_time gets set and the cooldown period starts.
        // Without this, the general logic immediately re-enters Attack
        // (cooldown_ready is still true) and the enemy never leaves Attack.
        return EnemyBehaviorKind::Idle;
    }

    if distance <= defaults::ENEMY_ATTACK_RANGE && attack_cooldown_ready {
        EnemyBehaviorKind::Attack
    } else if distance <= defaults::ENEMY_ATTACK_DISENGAGE && !attack_cooldown_ready {
        // In or near attack range but cooldown not ready — hold position.
        EnemyBehaviorKind::Idle
    } else if distance > defaults::ENEMY_ATTACK_RANGE {
        // Beyond attack range — close the gap.
        EnemyBehaviorKind::Chase
    } else {
        // In attack range, cooldown not ready — wait.
        EnemyBehaviorKind::Idle
    }
}

/// Choose the replicated visual state for an enemy.
///
/// Attack is authoritative. For non-attack states, derive locomotion from
/// actual planar movement instead of the AI decision so separation/knockback
/// cannot produce visible idle-sliding or rapid Idle<->Chase clip resets.
pub fn enemy_animation_state(decision: EnemyBehaviorKind, planar_speed: f32) -> EnemyBehaviorKind {
    if decision == EnemyBehaviorKind::Attack {
        EnemyBehaviorKind::Attack
    } else if planar_speed > defaults::ENEMY_ANIMATION_SPEED_EPSILON {
        EnemyBehaviorKind::Chase
    } else {
        EnemyBehaviorKind::Idle
    }
}

/// Shared enum for enemy AI decisions. Mirrors the client `EnemyBehavior`
/// component and the server `animation_state` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyBehaviorKind {
    Idle,
    Chase,
    Attack,
}

impl EnemyBehaviorKind {
    pub const IDLE: u8 = 0;
    pub const CHASE: u8 = 1;
    pub const ATTACK: u8 = 2;

    /// Convert to the u8 representation used in server DB rows.
    pub fn as_u8(self) -> u8 {
        match self {
            Self::Idle => Self::IDLE,
            Self::Chase => Self::CHASE,
            Self::Attack => Self::ATTACK,
        }
    }

    /// Parse from the server DB u8 representation.
    pub fn from_u8(v: u8) -> Self {
        match v {
            Self::CHASE => Self::Chase,
            Self::ATTACK => Self::Attack,
            _ => Self::Idle,
        }
    }

    /// Convert to the string representation (for player animation protocol).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Chase => "Chase",
            Self::Attack => "Attack",
        }
    }

    /// Parse from the string representation.
    pub fn parse_str(s: &str) -> Self {
        match s {
            "Chase" => Self::Chase,
            "Attack" => Self::Attack,
            _ => Self::Idle,
        }
    }
}

/// Player animation state encoding for the server DB (u8 wire format).
/// Mirrors `AnimationState` on the client — both sides use these constants.
pub mod player_anim_state {
    pub const IDLE: u8 = 0;
    pub const WALK: u8 = 1;
    pub const RUN: u8 = 2;
    pub const CROUCH: u8 = 3;
    pub const CROUCH_IDLE: u8 = 4;
    pub const JUMP_START: u8 = 5;
    pub const JUMP: u8 = 6;
    pub const JUMP_LAND: u8 = 7;
    pub const FALL: u8 = 8;
    pub const ROLL: u8 = 9;
    pub const LANDING_STUN: u8 = 10;
    pub const KNOCK_BACK: u8 = 11;
}

/// Player attack animation encoding for the server DB (u8 wire format).
/// 0 = no attack, 1–3 = specific attack clips.
pub mod attack_anim {
    pub const NONE: u8 = 0;
    pub const PUNCH_JAB: u8 = 1;
    pub const PUNCH_CROSS: u8 = 2;
    pub const MELEE_HOOK: u8 = 3;
}

/// Enemy type encoding for the server DB.
pub mod enemy_types {
    pub const BASIC: u8 = 0;
    pub const FAST: u8 = 1;
    pub const BRUTE: u8 = 2;
}

/// Per-type enemy combat stats returned by [`enemy_defaults`].
#[derive(Debug, Clone, Copy)]
pub struct EnemyStats {
    pub health: f32,
    pub damage: f32,
    pub speed: f32,
    pub attack_range: f32,
    pub attack_speed: f32,
}

/// Returns the baseline stats for a given enemy type.
pub fn enemy_defaults(enemy_type: u8) -> EnemyStats {
    match enemy_type {
        enemy_types::FAST => EnemyStats {
            health: 200.0,
            damage: 10.0,
            speed: 4.0,
            attack_range: 2.0,
            attack_speed: 1.5,
        },
        enemy_types::BRUTE => EnemyStats {
            health: 1200.0,
            damage: 25.0,
            speed: 1.2,
            attack_range: 2.5,
            attack_speed: 0.5,
        },
        _ => EnemyStats {
            health: defaults::ENEMY_HEALTH,
            damage: defaults::ENEMY_ATTACK_DAMAGE,
            speed: defaults::ENEMY_WALK_SPEED,
            attack_range: defaults::ENEMY_ATTACK_RANGE,
            attack_speed: 1.0,
        },
    }
}

/// Effect type encoding for the server DB.
pub mod effect_types {
    pub const STACKING_DAMAGE: u8 = 0;
}

/// Attack timing constants (at 1.0x speed)
pub mod attack_timing {
    /// Base duration for punch animations (jab/cross)
    pub const PUNCH_DURATION: f32 = 0.42;
    /// Base duration for crit/hook animation (slower wind-up, bigger impact)
    pub const HOOK_DURATION: f32 = 0.55;
}

/// When the hit happens in each attack animation (fraction of duration)
pub mod hit_timing {
    pub const PUNCH_HIT_FRACTION: f32 = 0.55;
    pub const HOOK_HIT_FRACTION: f32 = 0.50;
}

/// Per-action feedback configuration, computed by rules.
#[derive(Debug, Clone, Default)]
pub struct HitFeedback {
    pub hit_stop_duration: f32,
    pub shake_intensity: f32,
    pub flash_duration: f32,
    pub rumble_strong: f32,
    pub rumble_weak: f32,
    pub rumble_duration: f32,
}

impl HitFeedback {
    /// Standard melee hit feedback values.
    pub fn standard(is_crit: bool) -> Self {
        Self {
            hit_stop_duration: 0.04,
            shake_intensity: 0.25,
            flash_duration: if is_crit { 0.15 } else { 0.08 },
            rumble_strong: 0.35,
            rumble_weak: 0.21,
            rumble_duration: 60.0,
        }
    }
}

/// Compute the position displacement for a knockback hit.
/// Both client (singleplayer) and server (multiplayer) call this to ensure
/// identical knockback behavior. Returns the world-space offset to apply.
///
/// TODO(server-physics): Replace with physics impulse once Avian3d runs on the
/// server. Both client and server will apply the same impulse; the physics
/// engine handles the smooth displacement. Delete this function at that point.
pub fn knockback_displacement(
    radial_dir: glam::Vec2,
    forward: glam::Vec2,
    knockback: f32,
    push: f32,
    launch: f32,
) -> glam::Vec3 {
    let xz = radial_dir * knockback + forward * push;
    glam::Vec3::new(xz.x, launch, xz.y)
}

/// 2D cone check on XZ plane. Returns true if target is within range and arc.
pub fn cone_hit_check(
    origin: glam::Vec2,
    forward: glam::Vec2,
    target: glam::Vec2,
    range: f32,
    half_arc_cos: f32,
) -> bool {
    let delta = target - origin;
    let dist = delta.length();

    if dist > range {
        return false;
    }

    if dist > 0.01 {
        let dir = delta / dist;
        let dot = forward.dot(dir);
        if dot < half_arc_cos {
            return false;
        }
    }

    true
}

/// Decay stacks to 0 if enough time has passed since last hit.
pub fn decay_stacks(stacks: f32, elapsed_secs: f64, decay_threshold: f32) -> f32 {
    if elapsed_secs > decay_threshold as f64 && stacks > 0.0 {
        0.0
    } else {
        stacks
    }
}

/// Check if enough time has passed since last attack (respecting attack speed).
pub fn can_attack(last_attack_micros: i64, now_micros: i64, attack_speed: f32) -> bool {
    let cooldown_micros =
        (defaults::ATTACK_COOLDOWN_SECS as f64 * 1_000_000.0 / attack_speed as f64) as i64;
    now_micros - last_attack_micros >= cooldown_micros
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enemy_animation_state_keeps_attack_authoritative() {
        assert_eq!(
            enemy_animation_state(EnemyBehaviorKind::Attack, 0.0),
            EnemyBehaviorKind::Attack
        );
    }

    #[test]
    fn enemy_animation_state_uses_motion_for_non_attack_states() {
        assert_eq!(
            enemy_animation_state(EnemyBehaviorKind::Idle, 0.5),
            EnemyBehaviorKind::Chase
        );
        assert_eq!(
            enemy_animation_state(EnemyBehaviorKind::Chase, 0.0),
            EnemyBehaviorKind::Idle
        );
    }
}

// ============================================================================
// AOE CONSTANTS — shared between client and server
// ============================================================================

/// Ground pound AOE constants.
pub mod ground_pound {
    pub const RADIUS: f32 = 6.0;
    pub const KNOCKBACK: f32 = 20.0;
    pub const LAUNCH: f32 = 8.0;
    /// Damage multiplier: ground pound deals 4x normal attack damage.
    pub const DAMAGE_MULTIPLIER: f32 = 4.0;
    /// Minimum downward velocity to allow ground pound activation.
    /// Low threshold so it triggers almost immediately after jump apex.
    pub const MIN_VELOCITY: f32 = 0.5;
}

/// Landing AOE constants — high-velocity landings damage nearby enemies.
pub mod landing_aoe {
    /// Minimum fall velocity to trigger landing AOE damage.
    pub const MIN_VELOCITY: f32 = 10.0;
    /// Fall velocity that gives maximum AOE radius and knockback.
    pub const MAX_VELOCITY: f32 = 25.0;
    pub const MIN_RADIUS: f32 = 3.0;
    pub const MAX_RADIUS: f32 = 8.0;
    pub const KNOCKBACK: f32 = 14.0;
    pub const LAUNCH: f32 = 6.0;
    pub const DAMAGE_MULTIPLIER: f32 = 8.0;

    /// Compute velocity-scaled AOE parameters. Returns (radius, knockback, launch).
    pub fn scaled_params(velocity_y: f32) -> (f32, f32, f32) {
        let t = ((velocity_y - MIN_VELOCITY) / (MAX_VELOCITY - MIN_VELOCITY)).clamp(0.0, 1.0);
        let radius = MIN_RADIUS + (MAX_RADIUS - MIN_RADIUS) * t;
        let kb = KNOCKBACK * (0.5 + 0.5 * t);
        let launch = LAUNCH * (0.5 + 0.5 * t);
        (radius, kb, launch)
    }
}
