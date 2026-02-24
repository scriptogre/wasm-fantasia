use bevy::{prelude::*, render::storage::ShaderStorageBuffer};

use crate::asset::RemapInfo;

/// Clip IDs are indices into `RemapInfo.animations` (sorted by key name).
/// Users define clip names at asset load time; the ID is the position in sorted order.
pub type ClipId = u8;

/// Component to control the playback of a VAT animation on an entity.
#[derive(Debug, Clone, Component, Reflect)]
pub struct VatAnimationController {
    pub remap_info: Handle<RemapInfo>,
    pub current_clip: ClipId,
    /// Reference time for animation start (Global time).
    pub start_time: f32,
    /// Accumulated time offset (used for handling pause/resume/looping).
    pub offset: f32,
    /// Playback speed multiplier (1.0 is normal speed).
    pub speed: f32,
    pub is_playing: bool,
}

impl Default for VatAnimationController {
    fn default() -> Self {
        Self {
            remap_info: Handle::default(),
            current_clip: 0,
            start_time: 0.0,
            offset: 0.0,
            speed: 1.0,
            is_playing: true,
        }
    }
}

/// CPU-side data prepared each frame for the compute pre-skinning pipeline.
/// Extracted to the render world by the compute node.
#[derive(Resource, Default, Clone)]
pub struct VatComputeInput {
    /// Unique absolute frame indices currently in use, one per slot.
    pub frame_table: Vec<u32>,
    /// Per-entity mapping: instance_lookup[mesh_tag] = frame slot index.
    pub instance_lookup: Vec<u32>,
    /// Number of active unique frames to dispatch.
    pub active_frame_count: u32,
}

/// GPU buffer handles and static parameters for the compute pre-skinning pipeline.
/// Created once by the client when VAT assets are loaded.
#[derive(Resource, Clone)]
pub struct VatComputeResources {
    pub vat_texture: Handle<Image>,
    pub vertex_uvs: Handle<ShaderStorageBuffer>,
    pub frame_table_buffer: Handle<ShaderStorageBuffer>,
    pub instance_lookup_buffer: Handle<ShaderStorageBuffer>,
    pub pre_skinned_buffer: Handle<ShaderStorageBuffer>,
    pub vertex_count: u32,
    pub min_pos: Vec3,
    pub max_pos: Vec3,
    pub range: Vec3,
    pub tex_height: u32,
    pub frame_count: u32,
}
