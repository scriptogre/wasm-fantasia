//! ReShade-style post-processing effects
//! Toggle with F2
use crate::*;
use bevy::render::view::{ColorGrading, ColorGradingGlobal, ColorGradingSection};

#[derive(Resource)]
pub struct PostFxEnabled(pub bool);

impl Default for PostFxEnabled {
    fn default() -> Self {
        Self(true)
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<PostFxEnabled>()
        .add_systems(OnEnter(Screen::Gameplay), setup_postfx)
        .add_systems(Update, toggle_postfx.run_if(in_state(Screen::Gameplay)));
}

/// "Clean & Sharp" preset inspired by ReShade community standards
fn postfx_preset() -> ColorGrading {
    ColorGrading {
        global: ColorGradingGlobal {
            exposure: 0.1,    // Slight brightness boost
            temperature: 0.0, // Neutral
            tint: 0.0,        // Neutral
            hue: 0.0,         // No hue shift
            ..default()
        },
        highlights: ColorGradingSection {
            saturation: 1.05, // Slightly more vivid highlights
            contrast: 1.1,    // More punch in brights
            gamma: 1.0,
            gain: 1.0,
            lift: 0.0,
        },
        midtones: ColorGradingSection {
            saturation: 1.15, // Vibrance boost (main color pop)
            contrast: 1.05,   // Subtle local contrast
            gamma: 0.98,      // Slightly darker mids for depth
            gain: 1.0,
            lift: 0.0,
        },
        shadows: ColorGradingSection {
            saturation: 0.95, // Slightly desaturated shadows (cinematic)
            contrast: 1.15,   // Deeper blacks (FakeHDR effect)
            gamma: 0.95,      // Crush blacks slightly
            gain: 1.0,
            lift: -0.02, // Lower shadow floor
        },
    }
}

fn setup_postfx(mut commands: Commands, camera: Query<Entity, With<SceneCamera>>) {
    let Ok(cam) = camera.single() else { return };

    commands.entity(cam).insert(postfx_preset());
    info!("Post-FX enabled (F2 to toggle)");
}

fn toggle_postfx(
    keys: Res<ButtonInput<KeyCode>>,
    mut enabled: ResMut<PostFxEnabled>,
    mut commands: Commands,
    camera: Query<Entity, With<SceneCamera>>,
) {
    if !keys.just_pressed(KeyCode::F2) {
        return;
    }

    enabled.0 = !enabled.0;
    let Ok(cam) = camera.single() else { return };

    if enabled.0 {
        commands.entity(cam).insert(postfx_preset());
        info!("Post-FX ON");
    } else {
        commands.entity(cam).insert(ColorGrading::default());
        info!("Post-FX OFF");
    }
}
