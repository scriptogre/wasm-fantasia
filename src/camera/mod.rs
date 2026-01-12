use crate::*;
use bevy::{
    anti_alias::{fxaa::Fxaa, taa::TemporalAntiAliasing},
    core_pipeline::prepass::DeferredPrepass,
    pbr::DefaultOpaqueRendererMethod,
    render::view::Hdr,
};

#[cfg(feature = "third_person")]
mod third_person;

pub fn plugin(app: &mut App) {
    app.insert_resource(DefaultOpaqueRendererMethod::deferred())
        .add_systems(Startup, spawn_camera)
        .add_systems(OnEnter(Screen::Title), add_skybox_to_camera);

    #[cfg(feature = "third_person")]
    app.add_plugins(third_person::plugin);
}

pub fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        SceneCamera,
        IsDefaultUiCamera,
        Camera3d::default(),
        Camera::default(),
        Transform::from_xyz(100., 50., 100.).looking_at(Vec3::ZERO, Vec3::Y),
        Hdr,
        DeferredPrepass,
        TemporalAntiAliasing::default(),
        Fxaa::default(),
    ));
}
