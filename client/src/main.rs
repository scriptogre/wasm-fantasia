// Disable console on Windows for non-dev builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use bevy::asset::load_internal_binary_asset;
#[cfg(target_arch = "wasm32")]
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::{app::App, asset::AssetMetaCheck, log, prelude::*};
use bevy_fix_cursor_unlock_web::prelude::*;

pub mod asset_loading;
pub mod audio;
pub mod camera;
pub mod combat;
pub mod game;
pub mod gpu_profiler;
pub use game_client_models as models;
pub use game_client_networking as networking;
pub mod player;
pub mod postfx;
pub mod rendering;
pub mod profiling;
pub mod rule_presets;
pub mod rules;
pub mod scene;
pub mod screens;
pub mod ui;

use asset_loading::{AudioSources, Models, ResourceHandles};
use audio::*;
use models::*;
use ui::*;

fn main() {
    let mut app = App::new();

    let window = WindowPlugin {
        primary_window: Some(Window {
            title: "WASM Fantasia".to_string(),
            fit_canvas_to_parent: true,
            ..default()
        }),
        ..default()
    };
    let assets = AssetPlugin {
        meta_check: AssetMetaCheck::Never,
        ..default()
    };
    let filter = "info,cosmic_text=info,calloop=off,symphonia=off,naga=off,wgpu=warn,wgpu_core=error,bevy_core_pipeline=error,bevy_pbr=error,bevy_dev_tools=warn".to_string();
    let log_level = log::LogPlugin {
        level: log::Level::TRACE,
        filter,
        custom_layer: profiling::system_profile_layer,
        ..Default::default()
    };

    // On WASM, request both WebGPU and WebGL2 backends so browsers without WebGPU (Firefox) fall back to WebGL2.
    // Bevy's default only requests BROWSER_WEBGPU when the webgpu feature is enabled, which breaks Firefox.
    #[cfg(target_arch = "wasm32")]
    let render = bevy::render::RenderPlugin {
        render_creation: RenderCreation::Automatic(WgpuSettings {
            backends: Some(
                bevy::render::settings::Backends::BROWSER_WEBGPU
                    | bevy::render::settings::Backends::GL,
            ),
            ..default()
        }),
        ..default()
    };
    #[cfg(not(target_arch = "wasm32"))]
    let render = bevy::render::RenderPlugin::default();

    app.insert_resource(ClearColor(ui::colors::VOID));
    app.add_plugins(
        DefaultPlugins
            .set(window)
            .set(assets)
            .set(log_level)
            .set(render),
    );
    app.add_plugins((
        bevy::diagnostic::FrameTimeDiagnosticsPlugin::default(),
        bevy::diagnostic::EntityCountDiagnosticsPlugin::default(),
    ));
    app.add_plugins(bevy_hanabi::HanabiPlugin);
    app.add_plugins(bevy_open_vat::prelude::OpenVatPlugin);

    // custom plugins. the order is important
    // be sure you use resources/types AFTER you add plugins that insert them
    app.add_plugins((
        FixPointerUnlockPlugin,
        audio::plugin,
        asset_loading::plugin,
        ui::plugin,
        game::plugin,
        gpu_profiler::plugin,
    ));

    app.add_plugins(networking::NetworkingPlugin);

    // override default font
    load_internal_binary_asset!(
        app,
        TextFont::default().font,
        "../assets/fonts/ChakraPetch-SemiBold.ttf",
        |bytes: &[u8], _path: String| { Font::try_from_bytes(bytes.to_vec()).unwrap() }
    );
    app.run();
}
