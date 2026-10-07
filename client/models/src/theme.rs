//! Dark neutral surfaces and IBM Plex Sans.
use bevy::prelude::*;

/// Fonts embedded at startup. Regular is the default [`TextFont`] font.
pub mod fonts {
    use bevy::{asset::uuid_handle, prelude::*};

    /// Display and sheet titles
    pub const LIGHT: Handle<Font> = uuid_handle!("7b47aa09-2f08-4dad-9b89-7e0a46e62f48");
    /// Emphasis and numbers over the scene
    pub const SEMIBOLD: Handle<Font> = uuid_handle!("fddaaeb5-abc2-4725-9ba0-dbc92527da68");
}

/// Size constants
pub mod size {
    use bevy::ui::Val;

    /// Body and button text
    pub const FONT_SIZE: f32 = 18.0;
    /// Secondary text: tables, HUD status, touch labels
    pub const CAPTION_SIZE: f32 = 15.0;
    /// Sheet titles
    pub const HEADER_SIZE: f32 = 32.0;
    /// Game title and death screen
    pub const DISPLAY_SIZE: f32 = 56.0;

    /// Minimum touch target height
    pub const BUTTON_HEIGHT: Val = Val::Px(44.0);
    /// Ink notch on the left edge of bars
    pub const MARKER: f32 = 0.0;

    /// Screen edge inset in px for HUD and touch controls
    pub const EDGE: f32 = 20.0;

    pub const HEALTH_BAR_WIDTH: f32 = 200.0;
    pub const HEALTH_BAR_HEIGHT: f32 = 4.0;
}

pub mod colors {
    use bevy::prelude::Color;

    pub const PAPER: Color = Color::srgb(0.0784, 0.0745, 0.0706);
    pub const INK: Color = Color::srgb(0.902, 0.8784, 0.8157);
    pub const INK_SOFT: Color = Color::srgb(0.6471, 0.6235, 0.5725);
    pub const TINT: Color = Color::srgb(0.149, 0.1451, 0.1333);
    pub const HOVER: Color = Color::srgb(0.1882, 0.1804, 0.1686);
    pub const LINE: Color = Color::srgba(0.9255, 0.8941, 0.8235, 0.1);
    pub const FILL: Color = Color::srgb(0.9137, 0.8941, 0.8392);
    pub const ALERT: Color = Color::srgb(0.698, 0.2275, 0.251);
    pub const AMBER: Color = Color::oklcha(0.82, 0.15, 78.0, 1.0);

    // ── Scene ───────────────────────────────────────────────────────
    pub const NEUTRAL10: Color = Color::oklcha(0.998, 0.0, 0.0, 1.0);
    pub const NEUTRAL100: Color = Color::oklcha(0.970, 0.0, 0.0, 1.0);
    pub const NEUTRAL200: Color = Color::oklcha(0.922, 0.0, 0.0, 1.0);
    pub const NEUTRAL900: Color = Color::oklcha(0.205, 0.0, 0.0, 1.0);
    pub const NEUTRAL920: Color = Color::oklcha(0.181, 0.0, 0.0, 1.0);
    pub const HEALTH_RED: Color = Color::srgb(0.816, 0.125, 0.125);
    /// Near-black void used for ClearColor and fog
    pub const VOID: Color = Color::oklcha(0.100, 0.0, 0.0, 1.0);
}
/// Colors for one interaction state
#[derive(Component, Clone, Debug, Reflect)]
pub struct Palette {
    pub text: Color,
    pub bg: Color,
    pub border: BorderColor,
}

impl Palette {
    pub fn new(text: Color, bg: Color, border: BorderColor) -> Self {
        Self { text, bg, border }
    }
}

/// Palette for widget interactions
/// Add this to an entity you want changing color properties
#[derive(Component, Clone, Debug, Reflect)]
pub struct PaletteSet {
    pub none: Palette,
    pub hovered: Palette,
    pub pressed: Palette,
    pub disabled: Palette,
}
impl Default for PaletteSet {
    fn default() -> Self {
        let border = BorderColor::all(Color::NONE);
        Self {
            none: Palette::new(colors::INK, colors::TINT, border),
            hovered: Palette::new(colors::INK, colors::HOVER, border),
            pressed: Palette::new(colors::PAPER, colors::FILL, border),
            disabled: Palette::new(colors::INK_SOFT, colors::TINT, border),
        }
    }
}

impl PaletteSet {
    pub fn ghost() -> Self {
        Self {
            none: Palette::new(colors::INK_SOFT, Color::NONE, BorderColor::all(Color::NONE)),
            ..Self::default()
        }
    }

    pub fn selected() -> Self {
        let fill = Palette::new(colors::PAPER, colors::FILL, BorderColor::all(Color::NONE));
        Self {
            none: fill.clone(),
            hovered: fill.clone(),
            pressed: fill,
            disabled: Self::default().disabled,
        }
    }
}
