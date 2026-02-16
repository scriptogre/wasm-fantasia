use crate::*;
#[cfg(not(target_arch = "wasm32"))]
use bevy::anti_alias::taa::TemporalAntiAliasing;
use bevy::{
    core_pipeline::prepass::DeferredPrepass,
    pbr::{DefaultOpaqueRendererMethod, DistanceFog, FogFalloff},
    render::view::Hdr,
};

mod assist;
mod juice;
mod third_person;

pub fn plugin(app: &mut App) {
    // Deferred rendering requires features not available on WebGL2
    #[cfg(not(target_arch = "wasm32"))]
    app.insert_resource(DefaultOpaqueRendererMethod::deferred());

    app.add_systems(Startup, spawn_camera);
    app.add_plugins((third_person::plugin, assist::plugin, juice::plugin));
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

    let mut cam = commands.spawn((
        SceneCamera,
        IsDefaultUiCamera,
        Camera3d::default(),
        Camera::default(),
        Transform::from_xyz(100., 50., 100.).looking_at(Vec3::ZERO, Vec3::Y),
        Hdr,
        // Fog to fade grid into void at distance - creates infinite feel
        DistanceFog {
            color: colors::VOID,
            falloff: fog_falloff,
            ..default()
        },
    ));

    // TAA and deferred prepass require features unavailable on WebGL2
    #[cfg(not(target_arch = "wasm32"))]
    cam.insert((DeferredPrepass, TemporalAntiAliasing::default()));
}
