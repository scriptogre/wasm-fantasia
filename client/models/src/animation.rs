use bevy::prelude::*;

/// GLTF animation clips the game uses. Single source of truth for both local and remote players.
/// Unused clips are skipped during loading to save memory (especially on WASM).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect)]
pub enum Animation {
    Idle,
    JogFwd,
    Sprint,
    JumpStart,
    JumpLand,
    JumpLoop,
    NinjaJumpStart,
    NinjaJumpIdle,
    NinjaJumpLand,
    Roll,
    CrouchFwd,
    CrouchIdle,
    HitChest,
    PunchJab,
    PunchCross,
    MeleeHook,
    ZombieIdle,
    ZombieWalkForward,
    ZombieScratch,
}

impl Animation {
    /// All variants — used for loading and validation.
    pub const ALL: &[Animation] = &[
        Self::Idle,
        Self::JogFwd,
        Self::Sprint,
        Self::JumpStart,
        Self::JumpLand,
        Self::JumpLoop,
        Self::NinjaJumpStart,
        Self::NinjaJumpIdle,
        Self::NinjaJumpLand,
        Self::Roll,
        Self::CrouchFwd,
        Self::CrouchIdle,
        Self::HitChest,
        Self::PunchJab,
        Self::PunchCross,
        Self::MeleeHook,
        Self::ZombieIdle,
        Self::ZombieWalkForward,
        Self::ZombieScratch,
    ];

    /// Maps to the clip name inside the GLTF file.
    pub fn clip_name(self) -> &'static str {
        match self {
            Self::Idle => "Idle_Loop",
            Self::JogFwd => "Jog_Fwd_Loop",
            Self::Sprint => "Sprint_Loop",
            Self::JumpStart => "Jump_Start",
            Self::JumpLand => "Jump_Land",
            Self::JumpLoop => "Jump_Loop",
            Self::NinjaJumpStart => "NinjaJump_Start",
            Self::NinjaJumpIdle => "NinjaJump_Idle_Loop",
            Self::NinjaJumpLand => "NinjaJump_Land",
            Self::Roll => "Roll",
            Self::CrouchFwd => "Crouch_Fwd_Loop",
            Self::CrouchIdle => "Crouch_Idle_Loop",
            Self::HitChest => "Hit_Chest",
            Self::PunchJab => "Punch_Jab",
            Self::PunchCross => "Punch_Cross",
            Self::MeleeHook => "Melee_Hook",
            Self::ZombieIdle => "Zombie_Idle_Loop",
            Self::ZombieWalkForward => "Zombie_Walk_Fwd_Loop",
            Self::ZombieScratch => "Zombie_Scratch",
        }
    }

    /// Reverse lookup: GLTF clip name → enum variant.
    pub fn from_clip_name(name: &str) -> Option<Self> {
        Self::ALL.iter().find(|a| a.clip_name() == name).copied()
    }
}
