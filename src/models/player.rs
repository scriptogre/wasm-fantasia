use super::*;
use std::collections::HashMap;

#[derive(Component, Reflect, Clone)]
#[reflect(Component)]
pub struct Player {
    pub id: Entity,
    pub speed: f32,
    pub animation_state: AnimationState,
    pub animations: HashMap<String, AnimationNodeIndex>,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            // u32::MAX is Entity::PLACEHOLDER and using placeholder leeds to issues and using option
            // here while idiomatic will unnecessary complicate handling it in systems
            // We replace it with real id when the model is spawned anyway
            id: Entity::from_raw_u32(u32::MAX - 1).unwrap(),
            speed: 1.0,
            animation_state: AnimationState::StandIdle,
            animations: HashMap::new(),
        }
    }
}

#[derive(Component, Reflect, Default, Clone)]
#[reflect(Component)]
pub enum AnimationState {
    #[default]
    StandIdle,
    Run(f32),
    Sprint(f32),
    Climb(f32),
    JumpStart,
    JumpLoop,
    JumpLand,
    Fall,
    Crouch(f32),
    CrouchIdle,
    Dash,
    WallSlide,
    WallJump,
    KnockBack,
}
