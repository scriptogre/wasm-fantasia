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
/// Rebuilds buffer data every frame (cheap). Only dirties the material asset when
/// entity count changes — this triggers bind group recreation so the GPU sees the
/// resized buffer. In steady state, just writes buffer data without the expensive
/// material re-specialization cascade.
pub fn update_instance_data(
    mut commands: Commands,
    controller_query: Query<(Entity, &VatAnimationController, Option<&MeshTag>)>,
    mut materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, OpenVatExtension>>>,
    mat_query: Query<&MeshMaterial3d<ExtendedMaterial<StandardMaterial, OpenVatExtension>>>,
    remap_infos: Res<Assets<RemapInfo>>,
    mut buffers: ResMut<Assets<ShaderStorageBuffer>>,
    mut remap_events: MessageReader<AssetEvent<RemapInfo>>,
    mut last_count: Local<usize>,
    time: Res<Time>,
) {
    remap_events.clear();
    let current_count = controller_query.iter().len();
    let count_changed = *last_count != current_count;
    *last_count = current_count;

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
        let mut rate = speed / duration;
        let mut offset = -(controller.start_time * rate) + controller.offset;

        // Non-looping clips: clamp at last frame instead of wrapping.
        // The shader uses fract() which wraps all clips — freeze the
        // animation by setting rate=0 and offset to the end once elapsed
        // time exceeds the clip duration.
        if !clip.looping && speed > 0.0 {
            let elapsed = time.elapsed_secs() - controller.start_time;
            if elapsed >= duration / speed {
                let fc = (clip.end_frame - clip.start_frame).max(1) as f32;
                rate = 0.0;
                // Land on the last frame: (fc - 0.5) / fc rounds to fc,
                // giving absolute_frame = start + fc = end_frame.
                offset = (fc - 0.5) / fc;
            }
        }

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
    //
    // Only call materials.get_mut() when entity count changed — this marks the
    // material as AssetChanged, triggering bind group recreation (needed when
    // the buffer is resized for new entities). In steady state, just write buffer
    // data via buffers.get_mut() which is sufficient for GPU re-upload without
    // the expensive material re-specialization cascade.
    if count_changed {
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
    } else if let Some(mat_handle) = mat_query.iter().next() {
        // Steady state: write buffer data only (no material change).
        // All materials share the same buffer — one write is enough.
        if let Some(mat) = materials.get(&mat_handle.0)
            && let Some(buffer) = buffers.get_mut(&mat.extension.instance)
        {
            buffer.set_data(gpu_data_vec);
        }
    }
}
