use std::borrow::Cow;

use bevy::{
    prelude::*,
    render::{
        extract_resource::ExtractResource,
        render_asset::RenderAssets,
        render_graph::{Node, NodeRunError, RenderGraphContext, RenderLabel},
        render_resource::{
            binding_types, BindGroup, BindGroupEntries, BindGroupLayoutDescriptor,
            BindGroupLayoutEntries, CachedComputePipelineId, ComputePassDescriptor,
            ComputePipelineDescriptor, PipelineCache, ShaderStages, ShaderType,
            TextureSampleType, UniformBuffer,
        },
        renderer::{RenderContext, RenderDevice, RenderQueue},
        storage::GpuShaderStorageBuffer,
        texture::GpuImage,
    },
};

use crate::{
    data::{VatComputeInput, VatComputeResources},
    plugin::OPENVAT_COMPUTE_SHADER_HANDLE,
};

/// Render graph label for the VAT compute pre-skinning node.
#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub struct VatComputeLabel;

/// Cached compute pipeline + bind group layout descriptor.
#[derive(Resource)]
pub struct VatComputePipeline {
    layout: BindGroupLayoutDescriptor,
    pipeline_id: CachedComputePipelineId,
}

/// Bind group + backing uniform buffer for the current frame's compute dispatch.
#[derive(Resource)]
pub struct VatComputeBindGroup {
    bind_group: BindGroup,
    // Keep the uniform buffer alive so the bind group's reference stays valid.
    _uniform_buffer: UniformBuffer<VatComputeParams>,
}

/// Extracted copies of compute resources in the render world.
#[derive(Resource, Clone)]
pub struct ExtractedVatComputeInput(pub VatComputeInput);

impl ExtractResource for ExtractedVatComputeInput {
    type Source = VatComputeInput;
    fn extract_resource(source: &Self::Source) -> Self {
        ExtractedVatComputeInput(source.clone())
    }
}

#[derive(Resource, Clone)]
pub struct ExtractedVatComputeResources(pub VatComputeResources);

impl ExtractResource for ExtractedVatComputeResources {
    type Source = VatComputeResources;
    fn extract_resource(source: &Self::Source) -> Self {
        ExtractedVatComputeResources(source.clone())
    }
}

/// Uniform params matching the WGSL VatParams struct layout.
#[derive(Clone, Copy, Default, ShaderType)]
struct VatComputeParams {
    min_pos: Vec3,
    frame_count: u32,
    max_pos: Vec3,
    tex_height: u32,
    range: Vec3,
    vertex_count: u32,
}

/// Initializes the compute pipeline (runs once in RenderStartup).
pub fn init_vat_compute_pipeline(mut commands: Commands, pipeline_cache: Res<PipelineCache>) {
    let entries = BindGroupLayoutEntries::sequential(
        ShaderStages::COMPUTE,
        (
            binding_types::texture_2d(TextureSampleType::Float { filterable: false }),
            binding_types::storage_buffer_read_only_sized(false, None), // vertex_uvs
            binding_types::storage_buffer_read_only_sized(false, None), // frame_table
            binding_types::uniform_buffer::<VatComputeParams>(false),   // params
            binding_types::storage_buffer_sized(false, None),           // pre_skinned (rw)
        ),
    );

    let layout = BindGroupLayoutDescriptor::new("VatComputeLayout", &entries);

    let pipeline_id = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
        label: Some(Cow::from("vat_compute_pipeline")),
        layout: vec![layout.clone()],
        shader: OPENVAT_COMPUTE_SHADER_HANDLE,
        entry_point: Some(Cow::from("preskin")),
        ..default()
    });

    commands.insert_resource(VatComputePipeline {
        layout,
        pipeline_id,
    });
}

/// Creates the bind group each frame from extracted GPU resources.
#[allow(clippy::too_many_arguments)]
pub fn prepare_vat_bind_group(
    mut commands: Commands,
    pipeline: Option<Res<VatComputePipeline>>,
    resources: Option<Res<ExtractedVatComputeResources>>,
    input: Option<Res<ExtractedVatComputeInput>>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    gpu_buffers: Res<RenderAssets<GpuShaderStorageBuffer>>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    pipeline_cache: Res<PipelineCache>,
) {
    let (Some(pipeline), Some(resources), Some(input)) = (pipeline, resources, input) else {
        return;
    };
    let resources = &resources.0;
    let input = &input.0;
    if input.active_frame_count == 0 {
        return;
    }

    let (Some(gpu_image), Some(vertex_uvs), Some(frame_table), Some(pre_skinned)) = (
        gpu_images.get(&resources.vat_texture),
        gpu_buffers.get(&resources.vertex_uvs),
        gpu_buffers.get(&resources.frame_table_buffer),
        gpu_buffers.get(&resources.pre_skinned_buffer),
    ) else {
        return;
    };

    // Write uniform buffer with static params
    let params = VatComputeParams {
        min_pos: resources.min_pos,
        frame_count: resources.frame_count,
        max_pos: resources.max_pos,
        tex_height: resources.tex_height,
        range: resources.range,
        vertex_count: resources.vertex_count,
    };

    let mut uniform_buffer = UniformBuffer::<VatComputeParams>::default();
    uniform_buffer.set(params);
    uniform_buffer.write_buffer(&render_device, &render_queue);

    let Some(uniform_binding) = uniform_buffer.binding() else {
        return;
    };

    let bind_group_layout = pipeline_cache.get_bind_group_layout(&pipeline.layout);

    let bind_group = render_device.create_bind_group(
        Some("vat_compute_bind_group"),
        &bind_group_layout,
        &BindGroupEntries::sequential((
            &gpu_image.texture_view,
            vertex_uvs.buffer.as_entire_buffer_binding(),
            frame_table.buffer.as_entire_buffer_binding(),
            uniform_binding,
            pre_skinned.buffer.as_entire_buffer_binding(),
        )),
    );

    commands.insert_resource(VatComputeBindGroup {
        bind_group,
        _uniform_buffer: uniform_buffer,
    });
}

/// Render graph node that dispatches the compute pre-skinning shader.
#[derive(Default)]
pub struct VatComputeNode;

impl Node for VatComputeNode {
    fn run<'w>(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        let Some(bind_group) = world.get_resource::<VatComputeBindGroup>() else {
            return Ok(());
        };
        let Some(input) = world.get_resource::<ExtractedVatComputeInput>() else {
            return Ok(());
        };
        let Some(resources) = world.get_resource::<ExtractedVatComputeResources>() else {
            return Ok(());
        };

        if input.0.active_frame_count == 0 {
            return Ok(());
        }

        let pipeline_cache = world.resource::<PipelineCache>();
        let pipeline = world.resource::<VatComputePipeline>();

        let Some(compute_pipeline) = pipeline_cache.get_compute_pipeline(pipeline.pipeline_id)
        else {
            return Ok(());
        };

        let mut pass = render_context
            .command_encoder()
            .begin_compute_pass(&ComputePassDescriptor {
                label: Some("vat_preskin"),
                ..default()
            });

        pass.set_pipeline(compute_pipeline);
        pass.set_bind_group(0, &bind_group.bind_group, &[]);

        let workgroups_y = (resources.0.vertex_count + 63) / 64;
        pass.dispatch_workgroups(input.0.active_frame_count, workgroups_y, 1);

        Ok(())
    }
}
