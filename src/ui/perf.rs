use super::*;
use bevy::dev_tools::fps_overlay::FpsOverlayPlugin;
// use iyes_perf_ui::{
//     PerfUiPlugin,
//     entries::{PerfUiFramerateEntries, PerfUiWindowEntries},
//     prelude::*,
// };

pub fn plugin(app: &mut App) {
    app.add_plugins((
        // PerfUiPlugin,
        FpsOverlayPlugin::default(),
        bevy::render::diagnostic::RenderDiagnosticsPlugin,
        bevy::diagnostic::EntityCountDiagnosticsPlugin::default(),
        // https://github.com/IyesGames/iyes_perf_ui/issues/30
        // bevy::diagnostic::SystemInformationDiagnosticsPlugin,
    ));

    app.add_systems(Startup, setup_perf_ui);
}

fn setup_perf_ui(mut commands: Commands) {
    commands.spawn((
        PerfUi,
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(10.0),
            top: Val::Px(10.0),
            ..default()
        },
        // PerfUiRoot {
        //     position: PerfUiPosition::TopRight,
        //     ..default()
        // },
        // // Contains everything related to FPS and frame time
        // PerfUiFramerateEntries::default(),
        // // Contains everything related to the window and cursor
        // PerfUiWindowEntries::default(),
        // // Contains everything related to system diagnostics (CPU, RAM)
        // PerfUiSystemEntries::default(),
    ));
}
