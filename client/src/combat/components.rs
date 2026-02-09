use bevy::prelude::*;

pub use wasm_fantasia_shared::combat::{attack_timing, hit_timing};

pub fn plugin(app: &mut App) {
    app.register_type::<Health>()
        .register_type::<AttackState>()
        .register_type::<EnemyBehavior>();
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
    pub cooldown: Timer,
    pub attacking: bool,
    pub attack_time: f32,
    pub attack_duration: f32,
    pub hit_time: f32,
    pub attack_count: u32,
    pub hit_triggered: bool,
    pub is_crit: bool,
}

impl AttackState {
    pub fn new(cooldown_secs: f32) -> Self {
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

/// Current behavior state for enemy AI and animation.
#[derive(Component, Default, Clone, Copy, PartialEq, Eq, Reflect, Debug)]
#[reflect(Component)]
pub enum EnemyBehavior {
    #[default]
    Idle,
    Chase,
    Attack,
}

/// Tracks the enemy's animation graph and currently playing clip.
#[derive(Component, Default)]
pub struct EnemyAnimations {
    pub animations: std::collections::HashMap<crate::player::Animation, AnimationNodeIndex>,
    pub animation_player_entity: Option<Entity>,
    pub current_animation: Option<crate::player::Animation>,
}

/// Enemy AI state (singleplayer only).
#[derive(Component)]
pub struct EnemyAi {
    pub attack_cooldown: Timer,
}

impl Default for EnemyAi {
    fn default() -> Self {
        Self {
            attack_cooldown: Timer::from_seconds(
                wasm_fantasia_shared::combat::defaults::ENEMY_ATTACK_COOLDOWN,
                TimerMode::Once,
            ),
        }
    }
}

/// Remaining knockback displacement to apply smoothly over frames.
/// TODO(server-physics): Remove once Avian3d runs on the server — knockback
/// becomes a physics impulse and the engine handles smooth movement natively.
#[derive(Component, Debug)]
pub struct KnockbackRemaining(pub Vec3);

