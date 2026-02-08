use super::*;
use bevy::dev_tools::fps_overlay::{FpsOverlayConfig, FpsOverlayPlugin, FrameTimeGraphConfig};
use std::time::Duration;

const FPS_OVERLAY_ZINDEX: i32 = i32::MAX - 32;

pub fn plugin(app: &mut App) {
    app.add_plugins(FpsOverlayPlugin {
        config: FpsOverlayConfig {
            text_config: TextFont {
                font_size: 14.0,
                ..default()
            },
            text_color: colors::NEUTRAL400.into(),
            enabled: true,
            refresh_interval: Duration::from_millis(100),
            frame_time_graph_config: FrameTimeGraphConfig {
                enabled: false,
                ..default()
            },
        },
    });

    app.add_systems(PostStartup, (strip_fps_label, adjust_fps_layout));
}

/// Remove the "FPS: " prefix — just show the number.
fn strip_fps_label(mut texts: Query<&mut Text>) {
    for mut text in &mut texts {
        if text.0.starts_with("FPS:") {
            text.0 = String::new();
        }
    }
}

/// Add left padding to the FPS overlay to align with the debug panel.
fn adjust_fps_layout(mut nodes: Query<(&GlobalZIndex, &mut Node)>) {
    for (z, mut node) in &mut nodes {
        if z.0 == FPS_OVERLAY_ZINDEX {
            node.left = Val::Px(16.0);
            node.top = Val::Px(16.0);
        }
    }
}

