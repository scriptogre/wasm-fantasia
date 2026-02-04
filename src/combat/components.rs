use avian3d::prelude::PhysicsLayer;
use bevy::{animation::AnimationEvent, prelude::*};

pub fn plugin(app: &mut App) {
    app.register_type::<Health>()
        .register_type::<AttackState>();
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

/// Tracks attack state for entities that can attack.
#[derive(Component, Reflect, Debug, Clone, Default)]
#[reflect(Component)]
pub struct AttackState {
    /// Time remaining before another attack can be performed.
    pub cooldown: Timer,
    /// Whether we're currently in an attack animation.
    pub attacking: bool,
    /// Frame within the attack (for animation speed control).
    pub attack_frame: u32,
    /// Counter for alternating attack animations.
    pub attack_count: u32,
    /// Whether the hit has been triggered for this attack.
    pub hit_triggered: bool,
    /// Whether current attack is a critical hit.
    pub is_crit: bool,
}

impl AttackState {
    pub fn new(cooldown_secs: f32) -> Self {
        Self {
            cooldown: Timer::from_seconds(cooldown_secs, TimerMode::Once),
            attacking: false,
            attack_frame: 0,
            attack_count: 0,
            hit_triggered: false,
            is_crit: false,
        }
    }

    pub fn can_attack(&self) -> bool {
        self.cooldown.is_finished() && !self.attacking
    }

    pub fn start_attack(&mut self) {
        self.attacking = true;
        self.attack_frame = 0;
        self.attack_count += 1;
        self.hit_triggered = false;
        self.cooldown.reset();
    }
}

/// Marker for entities currently being knocked back.
#[derive(Component, Reflect, Debug, Clone, Default)]
#[reflect(Component)]
pub struct Staggered {
    pub duration: Timer,
}

/// Event fired when an entity takes damage (use with commands.trigger()).
#[derive(Event, Debug, Clone)]
pub struct DamageEvent {
    pub target: Entity,
    pub damage: f32,
    pub knockback_direction: Vec3,
    pub knockback_force: f32,
    pub is_crit: bool,
}

/// Event fired when an entity dies (use with commands.trigger()).
#[derive(Event, Debug, Clone)]
pub struct DeathEvent {
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

/// Animation event fired when an attack connects.
/// Added to attack animation clips at the frame where the hit lands.
#[derive(AnimationEvent, Clone, Debug)]
pub struct AttackConnect;

/// Physics collision layers for combat entities.
#[derive(PhysicsLayer, Clone, Copy, Debug, Default)]
pub enum GameLayer {
    #[default]
    Ground,
    Player,
    Enemy,
}
