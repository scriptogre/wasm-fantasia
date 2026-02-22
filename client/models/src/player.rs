use crate::animation::Animation;
use bevy::animation::AnimationEvent;
use bevy::prelude::*;
use std::collections::HashMap;

/// Animation event fired when a foot contacts the ground during locomotion clips.
/// Injected into Walk and JogFwd clips at load time so it fires for any entity
/// playing those animations (local or remote).
#[derive(AnimationEvent, Clone)]
pub struct FootContact;

/// Foot-contact fractions within one loop cycle (0.0–1.0).
/// Two contacts per cycle = two steps.
pub const JOG_FOOT_CONTACTS: &[f32] = &[0.1, 0.6];
pub const SPRINT_FOOT_CONTACTS: &[f32] = &[0.15, 0.65];

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
    /// Serialize to a u8 animation ID for broadcast to other clients.
    /// Speed parameters are dropped — remote players use default playback speeds.
    pub fn server_id(&self) -> u8 {
        use game_core::combat::player_anim_state as S;
        match self {
            Self::StandIdle => S::IDLE,
            Self::Run(_) | Self::Climb(_) => S::WALK,
            Self::Sprint(_) => S::RUN,
            Self::Crouch(_) => S::CROUCH,
            Self::CrouchIdle => S::CROUCH_IDLE,
            Self::JumpStart => S::JUMP_START,
            Self::JumpLoop | Self::WallJump => S::JUMP,
            Self::JumpLand => S::JUMP_LAND,
            Self::Fall | Self::WallSlide => S::FALL,
            Self::Roll => S::ROLL,
            Self::LandingStun => S::LANDING_STUN,
            Self::KnockBack => S::KNOCK_BACK,
            Self::Attack => S::IDLE, // Attacks handled by attack_sequence/attack_animation
            Self::GroundPound => S::FALL, // Diving pose for remote players
        }
    }

    /// Deserialize from a u8 animation ID. Speed-parameterized variants
    /// get default display speeds since the exact value isn't transmitted.
    pub fn from_server_id(id: u8) -> Self {
        use game_core::combat::player_anim_state as S;
        match id {
            S::WALK => Self::Run(1.0),
            S::RUN => Self::Sprint(1.0),
            S::CROUCH => Self::Crouch(1.0),
            S::CROUCH_IDLE => Self::CrouchIdle,
            S::JUMP_START => Self::JumpStart,
            S::JUMP => Self::JumpLoop,
            S::JUMP_LAND => Self::JumpLand,
            S::FALL => Self::Fall,
            S::ROLL => Self::Roll,
            S::LANDING_STUN => Self::LandingStun,
            S::KNOCK_BACK => Self::KnockBack,
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

/// Marker for remote (non-local) player entities.
#[derive(Component)]
pub struct RemotePlayer;
