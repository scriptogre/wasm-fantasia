use bevy::prelude::*;

pub use game_core::combat::{attack_timing, hit_timing};

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

impl EnemyBehavior {
    pub fn clip_name(&self) -> &'static str {
        match self {
            Self::Idle => "Zombie_Idle_Loop",
            Self::Chase => "Zombie_Walk_Fwd_Loop",
            Self::Attack => "Zombie_Scratch",
        }
    }
}

/// Queued knockback shove to apply on the next Tnua action feeding cycle.
/// Inserted by the damage observer, consumed by the knockback system that
/// runs after movement so `initiate_action_feeding()` has already been called.
#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
pub struct PendingKnockback(pub Vec3);

// ── Events ──────────────────────────────────────────────────────────

use game_core::combat::HitFeedback;

// ── Intent ──────────────────────────────────────────────────────────

/// Intent: the attack's hit frame was reached.
/// Resolved into [`DamageDealt`] + [`HitLanded`] per target.
#[derive(Event, Clone, Debug)]
pub struct AttackIntent {
    pub attacker: Entity,
}

// ── Mutations ───────────────────────────────────────────────────────

/// Mutation: damage was dealt to a target.
/// Caused by [`AttackIntent`] resolution. Triggers [`HitLanded`] and
/// potentially [`Died`].
#[derive(Event, Debug, Clone)]
pub struct DamageDealt {
    pub source: Entity,
    pub target: Entity,
    pub damage: f32,
    pub force: Vec3,
    pub is_crit: bool,
    pub feedback: HitFeedback,
}

/// Cross-domain mutation: an entity died.
/// Triggered by [`DamageDealt`] when health reaches zero.
#[derive(Event, Debug, Clone)]
pub struct Died {
    pub killer: Entity,
    pub entity: Entity,
}

// ── Feedback ────────────────────────────────────────────────────────

/// **Client-predicted** hit feedback — triggers sound, flash, hit stop, screen shake.
///
/// Fired immediately on client-side combat resolution for responsive feel.
/// **Does not** trigger damage numbers — those use server-confirmed
/// [`CombatEvent`](game_client_networking::CombatEvent) instead.
#[derive(Event, Debug, Clone)]
pub struct HitLanded {
    pub source: Entity,
    pub target: Entity,
    pub damage: f32,
    pub is_crit: bool,
    pub feedback: HitFeedback,
}

// ── Player control events (moved from player/control.rs) ────────────

/// Fired when the player lands a ground pound. Triggers AOE damage.
#[derive(Event)]
pub struct GroundPoundImpact {
    pub position: Vec3,
}

/// Fired when the player lands after being airborne. Impact scales with downward velocity.
#[derive(Event)]
pub struct LandingImpact {
    pub velocity_y: f32,
    pub position: Vec3,
}

// ── Stats system types ───────────────────────────────────────────────

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Stat keys for player/entity statistics.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Stat {
    Health,
    MaxHealth,
    AttackDamage,
    AbilityPower,
    Armor,
    MagicResist,
    AttackSpeed,
    MovementSpeed,
    CritChance,
    CritMultiplier,
    IsAttacking,
    AttackProgress,
    ComboCount,
    InWindup,
    InRecovery,
    Knockback,
    AttackRange,
    AttackArc,
    Stacks,
    StackDecay,
    Custom(String),
}

/// Bevy Component for entity stats — a simple `HashMap<Stat, f32>`.
#[derive(Component, Default, Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Stats(pub HashMap<Stat, f32>);

impl Stats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, stat: Stat, value: f32) -> Self {
        self.0.insert(stat, value);
        self
    }

    pub fn get(&self, stat: &Stat) -> f32 {
        self.0.get(stat).copied().unwrap_or(0.0)
    }

    pub fn set(&mut self, stat: Stat, value: f32) {
        self.0.insert(stat, value);
    }
}
