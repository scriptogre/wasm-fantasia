/// Player state stored on the server (authoritative).
#[spacetimedb::table(accessor = player, public)]
pub struct Player {
    #[primary_key]
    pub identity: spacetimedb::Identity,
    pub name: Option<String>,
    pub online: bool,
    pub world_id: u32,
    pub last_update: i64,

    // Position
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation_y: f32,

    // Animation
    pub animation_state: u8,
    pub attack_sequence: u32,
    pub attack_animation: u8,

    // Health
    pub health: f32,
    pub max_health: f32,

    // Combat
    pub attack_damage: f32,
    pub crit_chance: f32,
    pub crit_multiplier: f32,
    pub attack_range: f32,
    pub attack_arc: f32,
    pub knockback_force: f32,
    pub attack_speed: f32,
    pub last_attack_time: i64,
}

/// Server-authoritative enemy.
#[spacetimedb::table(accessor = enemy, public)]
pub struct Enemy {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub enemy_type: u8,
    #[index(btree)]
    pub world_id: u32,

    // Position
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation_y: f32,

    // Velocity (for physics-based movement and knockback)
    pub velocity_x: f32,
    pub velocity_y: f32,
    pub velocity_z: f32,

    // Animation
    pub animation_state: u8,
    /// Timestamp (micros since epoch) when current animation_state began.
    pub state_start_time: i64,

    // Health
    pub health: f32,
    pub max_health: f32,

    // Combat
    pub attack_damage: f32,
    pub attack_range: f32,
    pub attack_speed: f32,
    pub last_attack_time: i64,
}

/// Ephemeral hit notification — broadcast to subscribed clients, then auto-deleted.
#[spacetimedb::table(accessor = combat_event, public, event)]
pub struct CombatEvent {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub damage: f32,
    pub is_crit: bool,
    pub world_id: u32,
    pub timestamp: i64,
}

/// Dynamic effect (buff, debuff, DoT). Managed by combat reducers now,
/// by Rhai/Lua scripts later.
#[spacetimedb::table(accessor = active_effect, public)]
pub struct ActiveEffect {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub owner: spacetimedb::Identity,
    pub effect_type: u8,
    pub magnitude: f32,
    pub duration: f32,
    pub timestamp: i64,
}

/// Scheduled tick for server-side game logic (enemy AI, etc.).
#[spacetimedb::table(accessor = tick_schedule, scheduled(crate::enemy_ai::game_tick))]
pub struct TickSchedule {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: spacetimedb::ScheduleAt,
}

/// Knockback impulse to be applied to an enemy during the next physics tick.
/// Inserted by combat reducers, consumed by game_tick.
#[spacetimedb::table(accessor = knockback_impulse)]
pub struct KnockbackImpulse {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub enemy_id: u64,
    pub world_id: u32,
    pub impulse_x: f32,
    pub impulse_y: f32,
    pub impulse_z: f32,
}

/// Tracks which worlds are paused (singleplayer ESC menu).
#[spacetimedb::table(accessor = world_pause, public)]
pub struct WorldPause {
    #[primary_key]
    pub world_id: u32,
}

/// Horde spawner state — one row per world, drives automatic enemy spawning.
#[spacetimedb::table(accessor = horde_state)]
pub struct HordeState {
    #[primary_key]
    pub world_id: u32,
    pub active: bool,
    pub elapsed_secs: f32,
    pub spawn_accumulator: f32,
}
