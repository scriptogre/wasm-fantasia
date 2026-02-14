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
    Roll,
    LandingStun,
    WallSlide,
    WallJump,
    KnockBack,
    Attack,
    GroundPound,
}

impl AnimationState {
    /// Serialize to a server animation name for broadcast to other clients.
    /// Speed parameters are dropped — remote players use default playback speeds.
    pub fn server_name(&self) -> &'static str {
        match self {
            Self::StandIdle => "Idle",
            Self::Run(_) | Self::Climb(_) => "Walk",
            Self::Sprint(_) => "Run",
            Self::Crouch(_) => "Crouch",
            Self::CrouchIdle => "CrouchIdle",
            Self::JumpStart => "JumpStart",
            Self::JumpLoop | Self::WallJump => "Jump",
            Self::JumpLand => "JumpLand",
            Self::Fall | Self::WallSlide => "Fall",
            Self::Roll => "Roll",
            Self::LandingStun => "LandingStun",
            Self::KnockBack => "KnockBack",
            Self::Attack => "Idle", // Attacks handled by attack_sequence/attack_animation
            Self::GroundPound => "Fall", // Diving pose for remote players
        }
    }

    /// Deserialize from a server animation name. Speed-parameterized variants
    /// get default display speeds since the exact value isn't transmitted.
    pub fn from_server_name(name: &str) -> Self {
        match name {
            "Walk" => Self::Run(1.0),
            "Run" => Self::Sprint(1.0),
            "Crouch" => Self::Crouch(1.0),
            "CrouchIdle" => Self::CrouchIdle,
            "JumpStart" => Self::JumpStart,
            "Jump" => Self::JumpLoop,
            "JumpLand" => Self::JumpLand,
            "Fall" => Self::Fall,
            "Roll" => Self::Roll,
            "LandingStun" => Self::LandingStun,
            "KnockBack" => Self::KnockBack,
            _ => Self::StandIdle,
        }
    }

    /// Canonical clip, speed, and looping for display. This is the single source
    /// of truth for which animation clip plays for a given state — used by both
    /// the local player's animation system and remote player replication.
    pub fn playback(&self) -> (Animation, f32, bool) {
        match self {
            Self::StandIdle => (Animation::Idle, 1.0, true),
            Self::Run(_) => (Animation::JogFwd, 1.0, true),
            Self::Sprint(_) => (Animation::Sprint, 1.0, true),
            Self::JumpStart => (Animation::NinjaJumpStart, 1.5, false),
            Self::JumpLoop => (Animation::NinjaJumpIdle, 1.0, true),
            Self::JumpLand => (Animation::NinjaJumpLand, 1.5, false),
            Self::Fall => (Animation::NinjaJumpIdle, 1.0, true),
            Self::Crouch(_) => (Animation::CrouchFwd, 1.0, true),
            Self::CrouchIdle => (Animation::CrouchIdle, 1.0, true),
            Self::Roll => (Animation::Roll, 2.0, false),
            Self::LandingStun => (Animation::NinjaJumpLand, 1.2, false),
            Self::WallSlide => (Animation::NinjaJumpIdle, 1.0, true),
            Self::WallJump => (Animation::NinjaJumpStart, 2.0, false),
            Self::Climb(_) => (Animation::NinjaJumpIdle, 1.0, true),
            Self::KnockBack => (Animation::HitChest, 1.0, false),
            Self::Attack => (Animation::Idle, 1.0, true),
            Self::GroundPound => (Animation::NinjaJumpStart, 1.5, false),
        }
    }
}
