use bevy::asset::{load_internal_asset, uuid_handle};
use bevy::render::extract_resource::ExtractResourcePlugin;
use bevy::render::render_graph::RenderGraph;
use bevy::render::{Render, RenderApp, RenderSystems, RenderStartup};
use bevy::{pbr::ExtendedMaterial, prelude::*};

use crate::asset::{RemapInfo, RemapInfoAssetLoader};
use crate::compute::{
    ExtractedVatComputeInput, ExtractedVatComputeResources, VatComputeLabel, VatComputeNode,
    init_vat_compute_pipeline, prepare_vat_bind_group,
};
use crate::data::VatComputeInput;
use crate::material::OpenVatExtension;
use crate::system::prepare_vat_compute;

pub const OPENVAT_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("d4909c30-b350-4ae2-b003-b03b7adcb66d");
pub const OPENVAT_PREPASS_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("460afec2-b2d7-4ba3-9858-026997e63d4d");
pub const OPENVAT_COMPUTE_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("a1b2c3d4-e5f6-7890-abcd-ef1234567890");

/// Plugin that sets up the VAT material extension and compute pre-skinning pipeline.
pub struct OpenVatPlugin;

impl Plugin for OpenVatPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            OPENVAT_SHADER_HANDLE,
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/shaders/openvat_pbr.wgsl"
            ),
            Shader::from_wgsl
        );

        load_internal_asset!(
            app,
            OPENVAT_PREPASS_SHADER_HANDLE,
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/shaders/openvat_prepass.wgsl"
            ),
            Shader::from_wgsl
        );

        load_internal_asset!(
            app,
            OPENVAT_COMPUTE_SHADER_HANDLE,
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/shaders/openvat_compute.wgsl"
            ),
            Shader::from_wgsl
        );

        type MatPlugin = MaterialPlugin<ExtendedMaterial<StandardMaterial, OpenVatExtension>>;
        app.init_asset::<RemapInfo>()
            .register_asset_loader(RemapInfoAssetLoader)
            .add_plugins(MatPlugin::default())
            .init_resource::<VatComputeInput>()
            .add_systems(PostUpdate, prepare_vat_compute);

        // Extract resources to render world
        app.add_plugins((
            ExtractResourcePlugin::<ExtractedVatComputeInput>::default(),
            ExtractResourcePlugin::<ExtractedVatComputeResources>::default(),
        ));

        // Render world setup
        let render_app = app.sub_app_mut(RenderApp);
        render_app
            .add_systems(RenderStartup, init_vat_compute_pipeline)
            .add_systems(
                Render,
                prepare_vat_bind_group.in_set(RenderSystems::PrepareBindGroups),
            );

        // Add compute node to render graph, ordered before camera rendering
        let mut render_graph = render_app.world_mut().resource_mut::<RenderGraph>();
        render_graph.add_node(VatComputeLabel, VatComputeNode);
        render_graph.add_node_edge(VatComputeLabel, bevy::render::graph::CameraDriverLabel);
    }
}
