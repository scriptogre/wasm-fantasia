use bevy::prelude::*;

pub fn plugin(app: &mut App) {
    app.register_type::<Health>().register_type::<AttackState>();
}

/// Health component for any entity that can take damage.
#[derive(Component, Reflect, Debug, Clone)]
#[reflect(Component)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

impl Health {
    pub fn new(max: f32) -> Self {
        Self { current: max, max }
    }

    pub fn take_damage(&mut self, amount: f32) -> bool {
        self.current = (self.current - amount).max(0.0);
        self.current <= 0.0
    }

    pub fn is_dead(&self) -> bool {
        self.current <= 0.0
    }

    pub fn fraction(&self) -> f32 {
        self.current / self.max
    }
}

impl Default for Health {
    fn default() -> Self {
        Self::new(100.0)
    }
}

/// Attack timing constants (at 1.0x speed)
pub mod attack_timing {
    /// Base duration for punch animations (jab/cross)
    pub const PUNCH_DURATION: f32 = 0.42;
    /// Base duration for crit/hook animation (slower wind-up, bigger impact)
    pub const HOOK_DURATION: f32 = 0.55;
}

/// Tracks attack state for entities that can attack.
/// Uses time-based tracking so animation, VFX, and state stay in sync
/// regardless of attack speed multiplier.
#[derive(Component, Reflect, Debug, Clone, Default)]
#[reflect(Component)]
pub struct AttackState {
    /// Time remaining before another attack can be performed.
    pub cooldown: Timer,
    /// Whether we're currently in an attack animation.
    pub attacking: bool,
    /// Scaled time elapsed in current attack (advances faster with speed_mult).
    pub attack_time: f32,
    /// Base duration for current attack (before speed scaling).
    pub attack_duration: f32,
    /// Time when hit should trigger (before speed scaling).
    pub hit_time: f32,
    /// Counter for alternating attack animations.
    pub attack_count: u32,
    /// Whether the hit has been triggered for this attack.
    pub hit_triggered: bool,
    /// Whether current attack is a critical hit.
    pub is_crit: bool,
}

/// When the hit happens in each attack animation (fraction of duration)
pub mod hit_timing {
    pub const PUNCH_HIT_FRACTION: f32 = 0.55; // Hit at 55% through punch
    pub const HOOK_HIT_FRACTION: f32 = 0.50; // Hit at 50% through hook
}

impl AttackState {
    pub fn new(cooldown_secs: f32) -> Self {
        // Start cooldown as finished so player can attack immediately
        let mut cooldown = Timer::from_seconds(cooldown_secs, TimerMode::Once);
        cooldown.tick(std::time::Duration::from_secs_f32(cooldown_secs));

        Self {
            cooldown,
            attacking: false,
            attack_time: 0.0,
            attack_duration: attack_timing::PUNCH_DURATION,
            hit_time: attack_timing::PUNCH_DURATION * hit_timing::PUNCH_HIT_FRACTION,
            attack_count: 0,
            hit_triggered: false,
            is_crit: false,
        }
    }

    pub fn can_attack(&self) -> bool {
        self.cooldown.is_finished() && !self.attacking
    }

    pub fn start_attack(&mut self, is_crit: bool) {
        self.attacking = true;
        self.attack_time = 0.0;
        if is_crit {
            self.attack_duration = attack_timing::HOOK_DURATION;
            self.hit_time = attack_timing::HOOK_DURATION * hit_timing::HOOK_HIT_FRACTION;
        } else {
            self.attack_duration = attack_timing::PUNCH_DURATION;
            self.hit_time = attack_timing::PUNCH_DURATION * hit_timing::PUNCH_HIT_FRACTION;
        };
        self.attack_count += 1;
        self.hit_triggered = false;
        self.is_crit = is_crit;
        self.cooldown.reset();
    }

    /// Progress through attack (0.0 to 1.0)
    pub fn progress(&self) -> f32 {
        if self.attack_duration > 0.0 {
            (self.attack_time / self.attack_duration).min(1.0)
        } else {
            1.0
        }
    }
}

/// Marker for entities currently being knocked back.
#[derive(Component, Reflect, Debug, Clone, Default)]
#[reflect(Component)]
pub struct Staggered {
    pub duration: Timer,
}

/// Per-action feedback configuration, computed by rules.
/// Values are set by feedback presets and modified by rules (e.g., crit amplify).
#[derive(Debug, Clone, Default)]
pub struct HitFeedback {
    /// Hit stop (freeze frame) duration in seconds.
    pub hit_stop_duration: f32,
    /// Screen shake intensity (0.0 to 1.0).
    pub shake_intensity: f32,
    /// Hit flash duration on target in seconds.
    pub flash_duration: f32,
    /// Gamepad rumble strong motor (0.0 to 1.0).
    pub rumble_strong: f32,
    /// Gamepad rumble weak motor (0.0 to 1.0).
    pub rumble_weak: f32,
    /// Gamepad rumble duration in milliseconds.
    pub rumble_duration: f32,
}

/// Event fired when an entity takes damage (use with commands.trigger()).
#[derive(Event, Debug, Clone)]
pub struct DamageEvent {
    pub source: Entity,
    pub target: Entity,
    pub damage: f32,
    /// Combined force vector (radial + forward + vertical components)
    pub force: Vec3,
    pub is_crit: bool,
    /// Feedback configuration for this hit.
    pub feedback: HitFeedback,
}

/// Event fired when an entity dies (use with commands.trigger()).
#[derive(Event, Debug, Clone)]
pub struct DeathEvent {
    pub killer: Entity,
    pub entity: Entity,
}

/// Marker component for entities that can deal damage.
#[derive(Component, Reflect, Debug, Clone, Default)]
#[reflect(Component)]
pub struct Combatant;

/// Tag to identify the player for combat purposes.
#[derive(Component, Reflect, Debug, Clone, Default)]
#[reflect(Component)]
pub struct PlayerCombatant;

/// Tag to identify enemies.
#[derive(Component, Reflect, Debug, Clone, Default)]
#[reflect(Component)]
pub struct Enemy;

/// Event fired when an attack connects (hit frame reached).
/// Triggered by tick_attack_state when attack_time reaches hit_time.
#[derive(Event, Clone, Debug)]
pub struct AttackConnect {
    pub attacker: Entity,
}
