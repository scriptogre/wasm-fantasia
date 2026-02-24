use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::{render_resource::AsBindGroup, storage::ShaderStorageBuffer},
    shader::ShaderRef,
};

use crate::plugin::{OPENVAT_PREPASS_SHADER_HANDLE, OPENVAT_SHADER_HANDLE};

pub type VatStandardMaterial = ExtendedMaterial<StandardMaterial, OpenVatExtension>;

/// A material extension that reads pre-skinned vertex data from compute output.
#[derive(Debug, Default, Clone, Asset, AsBindGroup, Reflect)]
pub struct OpenVatExtension {
    /// Pre-skinned vertex data (position + normal per slot×vertex).
    /// Written by compute shader each frame.
    #[storage(100, visibility(vertex), read_only)]
    pub pre_skinned: Handle<ShaderStorageBuffer>,

    /// Per-entity mapping: instance_lookup[mesh_tag] = frame slot.
    #[storage(101, visibility(vertex), read_only)]
    pub instance_lookup: Handle<ShaderStorageBuffer>,

    /// Vertex count packed as x component of UVec4 for uniform alignment.
    #[uniform(102, visibility(vertex))]
    pub vertex_count: UVec4,
}

impl MaterialExtension for OpenVatExtension {
    fn vertex_shader() -> ShaderRef {
        OPENVAT_SHADER_HANDLE.into()
    }

    fn prepass_vertex_shader() -> ShaderRef {
        OPENVAT_PREPASS_SHADER_HANDLE.into()
    }
}
