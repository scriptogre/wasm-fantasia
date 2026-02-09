use super::*;
use crate::player::Animation;
use std::collections::HashMap;

#[derive(Component, Reflect, Clone)]
#[reflect(Component)]
pub struct Player {
    pub id: Entity,
    pub speed: f32,
    pub animation_state: AnimationState,
    pub animations: HashMap<Animation, AnimationNodeIndex>,
    /// Entity of the AnimationPlayer descendant (set during prepare_animations)
    pub anim_player_entity: Option<Entity>,
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
            anim_player_entity: None,
        }
    }
}

#[derive(Component, Reflect, Default, Clone, Debug)]
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
    SlideStart,
    SlideLoop,
    SlideExit,
    WallSlide,
    WallJump,
    KnockBack,
    Attack,
}

impl AnimationState {
    /// Lossy mapping to a simplified server animation name.
    /// Blend weights and sub-states are collapsed to broad categories.
    pub fn server_name(&self) -> &'static str {
        match self {
            Self::StandIdle | Self::CrouchIdle => "Idle",
            Self::Run(_) | Self::Climb(_) => "Walk",
            Self::Sprint(_) | Self::SlideStart | Self::SlideLoop | Self::SlideExit => "Run",
            Self::Crouch(_) => "Crouch",
            Self::JumpStart | Self::JumpLoop | Self::WallJump => "Jump",
            Self::Fall | Self::JumpLand | Self::WallSlide => "Fall",
            Self::Attack | Self::KnockBack => "Idle",
        }
    }
}
