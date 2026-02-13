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

/// Phase of an attack's lifecycle. Ready → Windup → Recovery → Ready.
#[derive(Reflect, Debug, Clone, Default, PartialEq)]
pub enum AttackPhase {
    #[default]
    Ready,
    Windup {
        elapsed: f32,
        total_duration: f32,
        hit_time: f32,
    },
    Recovery {
        elapsed: f32,
        remaining_duration: f32,
        total_duration: f32,
    },
}

/// Tracks attack state for entities that can attack.
#[derive(Component, Reflect, Debug, Clone, Default)]
#[reflect(Component)]
pub struct AttackState {
    pub cooldown: Timer,
    pub phase: AttackPhase,
    pub attack_count: u32,
    pub is_crit: bool,
}

impl AttackState {
    pub fn new(cooldown_secs: f32) -> Self {
        let mut cooldown = Timer::from_seconds(cooldown_secs, TimerMode::Once);
        cooldown.tick(std::time::Duration::from_secs_f32(cooldown_secs));

        Self {
            cooldown,
            phase: AttackPhase::Ready,
            attack_count: 0,
            is_crit: false,
        }
    }

    pub fn is_attacking(&self) -> bool {
        !matches!(self.phase, AttackPhase::Ready)
    }

    pub fn in_windup(&self) -> bool {
        matches!(self.phase, AttackPhase::Windup { .. })
    }

    pub fn in_recovery(&self) -> bool {
        matches!(self.phase, AttackPhase::Recovery { .. })
    }

    pub fn can_attack(&self) -> bool {
        self.cooldown.is_finished() && !self.is_attacking()
    }

    pub fn start_attack(&mut self, is_crit: bool) {
        let (total_duration, hit_time) = if is_crit {
            (
                attack_timing::HOOK_DURATION,
                attack_timing::HOOK_DURATION * hit_timing::HOOK_HIT_FRACTION,
            )
        } else {
            (
                attack_timing::PUNCH_DURATION,
                attack_timing::PUNCH_DURATION * hit_timing::PUNCH_HIT_FRACTION,
            )
        };
        self.phase = AttackPhase::Windup {
            elapsed: 0.0,
            total_duration,
            hit_time,
        };
        self.attack_count += 1;
        self.is_crit = is_crit;
        self.cooldown.reset();
    }

    pub fn progress(&self) -> f32 {
        match &self.phase {
            AttackPhase::Ready => 0.0,
            AttackPhase::Windup {
                elapsed,
                total_duration,
                ..
            } => {
                if *total_duration > 0.0 {
                    (*elapsed / *total_duration).min(1.0)
                } else {
                    1.0
                }
            }
            AttackPhase::Recovery {
                elapsed,
                remaining_duration,
                total_duration,
            } => {
                let hit_time = *total_duration - *remaining_duration;
                if *total_duration > 0.0 {
                    ((hit_time + *elapsed) / *total_duration).min(1.0)
                } else {
                    1.0
                }
            }
        }
    }
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

/// Queued knockback shove to apply on the next Tnua action feeding cycle.
/// Inserted by the damage observer, consumed by the knockback system that
/// runs after movement so `initiate_action_feeding()` has already been called.
#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
pub struct PendingKnockback(pub Vec3);

