use super::*;

///    commands.spawn((
///     Mesh3d(meshes.add(Sphere::default())),
///     MeshMaterial3d(materials.add(ShieldMaterial {
///         color: LinearRgba::BLUE,
///         time: 0.0,
///         depletion: 0.5,
///         texture_low: Some(asset_server.load(LOW_NOISE_ASSET_PATH)),
///     })),
///     Transform::from_xyz(0.0, 0.0, 0.0),
/// ));
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct ShieldMaterial {
    #[uniform(0)]
    color: LinearRgba,
    #[uniform(1)]
    time: f32,
    #[uniform(2)]
    depletion: f32,
    #[texture(3)]
    #[sampler(4)]
    texture_low: Option<Handle<Image>>,
}

impl Material for ShieldMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER_ASSET_PATH.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }

    fn specialize(
        _pipeline: &MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}
