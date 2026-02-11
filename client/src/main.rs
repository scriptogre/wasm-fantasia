// Disable console on Windows for non-dev builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use bevy::asset::load_internal_binary_asset;
use bevy::{app::App, asset::AssetMetaCheck, log, prelude::*};
use bevy_fix_cursor_unlock_web::prelude::*;

pub mod asset_loading;
pub mod audio;
pub mod camera;
pub mod combat;
pub mod game;
pub mod models;
pub mod networking;
pub mod player;
pub mod postfx;
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
            title: "Bevy Game".to_string(),
            // Bind to canvas included in `index.html` for custom wasm js logic
            // canvas: Some("#bevy".to_owned()),
            fit_canvas_to_parent: true,
            // Tells wasm not to override default event handling, like F5 and Ctrl+R
            prevent_default_event_handling: false,
            ..default()
        }),
        ..default()
    };
    let assets = AssetPlugin {
        meta_check: AssetMetaCheck::Never,
        ..default()
    };
    // DEBUG
    // let filter = "debug,symphonia=off,naga=off,wgpu=warn,bevy_enhanced_input=debug".to_string();
    let filter = "info,cosmic_text=info,calloop=off,symphonia=off,naga=off,wgpu=warn,wgpu_core=error,bevy_core_pipeline=error,bevy_pbr=error,bevy_dev_tools=warn".to_string();
    let log_level = log::LogPlugin {
        level: log::Level::TRACE,
        filter,
        ..Default::default()
    };

    app.add_plugins(DefaultPlugins.set(window).set(assets).set(log_level));

    // custom plugins. the order is important
    // be sure you use resources/types AFTER you add plugins that insert them
    app.add_plugins((
        FixPointerUnlockPlugin,
        audio::plugin,
        asset_loading::plugin,
        ui::plugin,
        game::plugin,
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
