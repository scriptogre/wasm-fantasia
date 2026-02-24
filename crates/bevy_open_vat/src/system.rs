use bevy::{
    mesh::MeshTag,
    platform::collections::HashMap,
    prelude::*,
    render::storage::ShaderStorageBuffer,
};

use crate::{
    asset::RemapInfo,
    data::{VatAnimationController, VatComputeInput, VatComputeResources},
};

/// Prepares frame table and instance lookup for the compute pre-skinning pass.
#[allow(clippy::too_many_arguments)]
pub fn prepare_vat_compute(
    mut commands: Commands,
    controller_query: Query<(Entity, &VatAnimationController, Option<&MeshTag>)>,
    remap_infos: Res<Assets<RemapInfo>>,
    time: Res<Time>,
    mut input: ResMut<VatComputeInput>,
    resources: Option<Res<VatComputeResources>>,
    mut buffers: ResMut<Assets<ShaderStorageBuffer>>,
    mut frame_map: Local<HashMap<u32, u32>>,
) {
    let Some(resources) = resources else {
        return;
    };

    frame_map.clear();
    input.frame_table.clear();
    input.instance_lookup.clear();

    let current_count = controller_query.iter().len();
    input.instance_lookup.resize(current_count, 0);

    let now = time.elapsed_secs();

    for (index, (entity, controller, existing_tag)) in controller_query.iter().enumerate() {
        let target_tag = index as u32;
        if existing_tag.map_or(true, |tag| tag.0 != target_tag) {
            commands.entity(entity).insert(MeshTag(target_tag));
        }

        let Some(remap_info) = remap_infos.get(&controller.remap_info) else {
            continue;
        };
        let Some(clip) = remap_info.clip(controller.current_clip) else {
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
        let raw_progress = now * rate + offset;
        let progress = raw_progress.fract().abs();
        let clip_frame_count = clip.end_frame - clip.start_frame;
        let relative_frame = (progress * clip_frame_count as f32).round() as u32;
        let absolute_frame =
            clip.start_frame + relative_frame.min(clip_frame_count.saturating_sub(1));

        // Deduplicate: assign or reuse slot
        let next_slot = frame_map.len() as u32;
        let slot = *frame_map.entry(absolute_frame).or_insert_with(|| {
            input.frame_table.push(absolute_frame);
            next_slot
        });

        input.instance_lookup[index] = slot;
    }

    input.active_frame_count = input.frame_table.len() as u32;

    // Ensure non-empty buffers for valid GPU bind groups
    if input.frame_table.is_empty() {
        input.frame_table.push(0);
    }
    if input.instance_lookup.is_empty() {
        input.instance_lookup.push(0);
    }

    // Upload to GPU buffers
    if let Some(buffer) = buffers.get_mut(&resources.frame_table_buffer) {
        buffer.set_data(input.frame_table.clone());
    }
    if let Some(buffer) = buffers.get_mut(&resources.instance_lookup_buffer) {
        buffer.set_data(input.instance_lookup.clone());
    }
}
