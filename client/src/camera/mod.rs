use crate::*;
use bevy::{
    anti_alias::{fxaa::Fxaa, taa::TemporalAntiAliasing},
    core_pipeline::prepass::DeferredPrepass,
    pbr::{DefaultOpaqueRendererMethod, DistanceFog, FogFalloff},
    render::view::Hdr,
};

#[cfg(feature = "third_person")]
mod assist;
#[cfg(feature = "third_person")]
mod third_person;

pub fn plugin(app: &mut App) {
    app.insert_resource(DefaultOpaqueRendererMethod::deferred())
        .add_systems(Startup, spawn_camera);

    #[cfg(feature = "third_person")]
    app.add_plugins((third_person::plugin, assist::plugin));
}

pub fn spawn_camera(mut commands: Commands) {
    // Fog distance matches grid size (smaller for WASM)
    #[cfg(target_arch = "wasm32")]
    let fog_falloff = FogFalloff::Linear {
        start: 25.0,
        end: 55.0,
    };
    #[cfg(not(target_arch = "wasm32"))]
    let fog_falloff = FogFalloff::Linear {
        start: 50.0,
        end: 150.0,
    };

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
        // Fog to fade grid into void at distance - creates infinite feel
        DistanceFog {
            color: Color::srgb(0.92, 0.92, 0.95),
            falloff: fog_falloff,
            ..default()
        },
    ));
}
