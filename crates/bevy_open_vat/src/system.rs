use std::collections::HashSet;

use bevy::{
    mesh::MeshTag, pbr::ExtendedMaterial, prelude::*, render::storage::ShaderStorageBuffer,
};

use crate::{
    asset::RemapInfo,
    data::{VatAnimationController, VatInstanceData},
    material::OpenVatExtension,
};

/// Synchronizes the CPU-side animation state with the GPU via a storage buffer.
/// Rebuilds every frame for playing animations (start_time advances each frame).
pub fn update_instance_data(
    mut commands: Commands,
    controller_query: Query<(Entity, &VatAnimationController, Option<&MeshTag>)>,
    mut materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, OpenVatExtension>>>,
    mat_query: Query<&MeshMaterial3d<ExtendedMaterial<StandardMaterial, OpenVatExtension>>>,
    remap_infos: Res<Assets<RemapInfo>>,
    mut buffers: ResMut<Assets<ShaderStorageBuffer>>,
    mut remap_events: MessageReader<AssetEvent<RemapInfo>>,
) {
    remap_events.clear();
    let current_count = controller_query.iter().len();

    let mut gpu_data_vec: Vec<VatInstanceData> = Vec::with_capacity(current_count);

    for (index, (entity, controller, existing_tag)) in controller_query.iter().enumerate() {
        let target_tag_val = index as u32;

        let needs_tag_update = match existing_tag {
            Some(tag) => tag.0 != target_tag_val,
            None => true,
        };

        if needs_tag_update {
            commands.entity(entity).insert(MeshTag(target_tag_val));
        }

        let Some(remap_info) = remap_infos.get(&controller.remap_info) else {
            // Fill dummy data to keep index alignment if asset not ready
            gpu_data_vec.push(VatInstanceData::default());
            commands.entity(entity).insert(MeshTag(index as u32));
            continue;
        };
        let Some(clip) = remap_info.clip(controller.current_clip) else {
            gpu_data_vec.push(VatInstanceData::default());
            commands.entity(entity).insert(MeshTag(index as u32));
            continue;
        };

        let duration = clip.duration().unwrap_or(1.0);
        let speed = if controller.is_playing {
            controller.speed
        } else {
            0.0
        };
        let rate = speed / duration;
        let offset = -(controller.start_time * rate) + controller.offset;

        gpu_data_vec.push(VatInstanceData {
            start_frame: clip.start_frame,
            frame_count: clip.end_frame - clip.start_frame,
            rate,
            offset,
        });
    }

    if gpu_data_vec.is_empty() {
        return;
    }

    // Write buffer data. Deduplicate by material handle to avoid redundant
    // writes (all 5000 entities share 1-2 materials pointing to the same buffer).
    // Always use get_mut() on the material to mark it as AssetChanged — this
    // forces bind group recreation, ensuring the GPU references the current
    // buffer allocation (which changes when the buffer is resized).
    let mut seen: HashSet<AssetId<ExtendedMaterial<StandardMaterial, OpenVatExtension>>> =
        HashSet::new();
    for mat_handle in mat_query.iter() {
        if seen.insert(mat_handle.0.id())
            && let Some(mat) = materials.get_mut(&mat_handle.0)
            && let Some(buffer) = buffers.get_mut(&mat.extension.instance)
        {
            buffer.set_data(gpu_data_vec.clone());
        }
    }
}
