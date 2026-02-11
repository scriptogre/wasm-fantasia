/// Player state stored on the server (authoritative).
#[spacetimedb::table(name = player, public)]
pub struct Player {
    #[primary_key]
    pub identity: spacetimedb::Identity,
    pub name: Option<String>,
    pub online: bool,
    pub world_id: String,
    pub last_update: i64,

    // Position
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation_y: f32,

    // Animation
    pub animation_state: String,
    pub attack_sequence: u32,
    pub attack_animation: String,

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
#[spacetimedb::table(name = enemy, public)]
pub struct Enemy {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub enemy_type: String,
    pub world_id: String,

    // Position
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation_y: f32,

    // Animation
    pub animation_state: String,

    // Health
    pub health: f32,
    pub max_health: f32,

    // Combat
    pub attack_damage: f32,
    pub attack_range: f32,
    pub attack_speed: f32,
    pub last_attack_time: i64,
}

/// Ephemeral hit notification. Inserted by attack_hit, consumed by clients for VFX.
#[spacetimedb::table(name = combat_event, public)]
pub struct CombatEvent {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub damage: f32,
    pub is_crit: bool,
    pub world_id: String,
    pub timestamp: i64,
}

/// Dynamic effect (buff, debuff, DoT). Managed by combat reducers now,
/// by Rhai/Lua scripts later.
#[spacetimedb::table(name = active_effect, public)]
pub struct ActiveEffect {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub owner: spacetimedb::Identity,
    pub effect_type: String,
    pub magnitude: f32,
    pub duration: f32,
    pub timestamp: i64,
}

/// Scheduled tick for server-side game logic (enemy AI, etc.).
#[spacetimedb::table(name = tick_schedule, scheduled(crate::enemy_ai::game_tick))]
pub struct TickSchedule {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: spacetimedb::ScheduleAt,
}

/// Tracks which worlds are paused (singleplayer ESC menu).
#[spacetimedb::table(name = world_pause, public)]
pub struct WorldPause {
    #[primary_key]
    pub world_id: String,
}
