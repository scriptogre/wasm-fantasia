use rune::Any;

#[derive(Any, Debug, Clone)]
pub struct Combatant {
    #[rune(get, set)]
    pub id: u64,
    #[rune(get, set)]
    pub pos_x: f32,
    #[rune(get, set)]
    pub pos_y: f32,
    #[rune(get, set)]
    pub pos_z: f32,
    #[rune(get, set)]
    pub dir_x: f32,
    #[rune(get, set)]
    pub dir_z: f32,
    #[rune(get, set)]
    pub health: f32,
    #[rune(get, set)]
    pub max_health: f32,
    #[rune(get, set)]
    pub attack_damage: f32,
    #[rune(get, set)]
    pub crit_chance: f32,
    #[rune(get, set)]
    pub crit_multiplier: f32,
    #[rune(get, set)]
    pub knockback_force: f32,
    #[rune(get, set)]
    pub attack_range: f32,
    #[rune(get, set)]
    pub attack_arc: f32,
    #[rune(get, set)]
    pub attack_speed: f32,
    #[rune(get, set)]
    pub fury_stacks: i64,
    #[rune(get, set)]
    pub attack_speed_bonus: f32,
    #[rune(get, set)]
    pub cooldown_ready: bool,
    #[rune(get, set)]
    pub speed: f32,
}

#[derive(Any, Debug, Clone)]
#[rune(constructor)]
pub struct Hit {
    #[rune(get, set)]
    pub damage: f32,
    #[rune(get, set)]
    pub knockback: f32,
    #[rune(get, set)]
    pub is_crit: bool,
}
