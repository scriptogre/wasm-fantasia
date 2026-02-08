//! Various non-themable constants
use super::*;

/// Font asset paths (Chakra Petch family)
pub mod fonts {
    pub const REGULAR: &str = "fonts/ChakraPetch-Regular.ttf";
    pub const MEDIUM: &str = "fonts/ChakraPetch-Medium.ttf";
    pub const SEMIBOLD: &str = "fonts/ChakraPetch-SemiBold.ttf";
    pub const BOLD: &str = "fonts/ChakraPetch-Bold.ttf";
}

/// Size constants
pub mod size {
    use super::*;

    /// Default font size
    pub const FONT_SIZE: f32 = 20.0;

    /// Default border radius for buttons, panels, sliders, spinners, etc.
    pub const BORDER_RADIUS: Val = Vw(0.6);

    /// Common row size for buttons, sliders, spinners, etc.
    pub const ROW_HEIGHT: Val = Px(24.0);

    /// Width and height of a checkbox
    pub const CHECKBOX_SIZE: Val = Px(18.0);

    /// Width and height of a radio button
    pub const RADIO_SIZE: Val = Px(18.0);

    /// Width of a toggle switch
    pub const TOGGLE_WIDTH: Val = Px(32.0);

    /// Height of a toggle switch
    pub const TOGGLE_HEIGHT: Val = Px(18.0);

    /// Health bar width
    pub const HEALTH_BAR_WIDTH: f32 = 288.0;

    /// Health bar height
    pub const HEALTH_BAR_HEIGHT: f32 = 16.0;
}

/// Tailwind CSS neutral palette (oklch, zero chroma)
pub mod colors {
    use bevy::prelude::Color;

    // ── Neutral scale ───────────────────────────────────────────────
    pub const NEUTRAL10: Color = Color::oklcha(0.998, 0.0, 0.0, 1.0);
    pub const NEUTRAL25: Color = Color::oklcha(0.995, 0.0, 0.0, 1.0);
    pub const NEUTRAL50: Color = Color::oklcha(0.985, 0.0, 0.0, 1.0);
    pub const NEUTRAL75: Color = Color::oklcha(0.978, 0.0, 0.0, 1.0);
    pub const NEUTRAL100: Color = Color::oklcha(0.970, 0.0, 0.0, 1.0);
    pub const NEUTRAL150: Color = Color::oklcha(0.956, 0.0, 0.0, 1.0);
    pub const NEUTRAL200: Color = Color::oklcha(0.922, 0.0, 0.0, 1.0);
    pub const NEUTRAL300: Color = Color::oklcha(0.870, 0.0, 0.0, 1.0);
    pub const NEUTRAL350: Color = Color::oklcha(0.809, 0.0, 0.0, 1.0);
    pub const NEUTRAL400: Color = Color::oklcha(0.708, 0.0, 0.0, 1.0);
    pub const NEUTRAL450: Color = Color::oklcha(0.629, 0.0, 0.0, 1.0);
    pub const NEUTRAL500: Color = Color::oklcha(0.556, 0.0, 0.0, 1.0);
    pub const NEUTRAL550: Color = Color::oklcha(0.497, 0.0, 0.0, 1.0);
    pub const NEUTRAL600: Color = Color::oklcha(0.439, 0.0, 0.0, 1.0);
    pub const NEUTRAL650: Color = Color::oklcha(0.405, 0.0, 0.0, 1.0);
    pub const NEUTRAL700: Color = Color::oklcha(0.371, 0.0, 0.0, 1.0);
    pub const NEUTRAL750: Color = Color::oklcha(0.320, 0.0, 0.0, 1.0);
    pub const NEUTRAL800: Color = Color::oklcha(0.269, 0.0, 0.0, 1.0);
    pub const NEUTRAL850: Color = Color::oklcha(0.237, 0.0, 0.0, 1.0);
    pub const NEUTRAL875: Color = Color::oklcha(0.221, 0.0, 0.0, 1.0);
    pub const NEUTRAL900: Color = Color::oklcha(0.205, 0.0, 0.0, 1.0);
    pub const NEUTRAL910: Color = Color::oklcha(0.193, 0.0, 0.0, 1.0);
    pub const NEUTRAL920: Color = Color::oklcha(0.181, 0.0, 0.0, 1.0);
    pub const NEUTRAL930: Color = Color::oklcha(0.169, 0.0, 0.0, 1.0);
    pub const NEUTRAL940: Color = Color::oklcha(0.157, 0.0, 0.0, 1.0);
    pub const NEUTRAL950: Color = Color::oklcha(0.145, 0.0, 0.0, 1.0);

    // ── Semantic aliases ────────────────────────────────────────────
    pub const TRANSPARENT: Color = Color::srgba(0.0, 0.0, 0.0, 0.0);
    // ── Accent colors ───────────────────────────────────────────────
    pub const SAND_YELLOW: Color = Color::srgb(205. / 255., 170. / 255., 109. / 255.);
    pub const ACID_GREEN: Color = Color::srgb(0.286, 0.878, 0.373);
    pub const GRASS_GREEN: Color = Color::oklcha(0.5866, 0.1543, 129.84, 1.0);
    pub const RED: Color = Color::oklcha(0.5232, 0.1404, 13.84, 1.0);
    pub const HEALTH_RED: Color = Color::srgb(0.816, 0.125, 0.125);

    // ── Scene ──────────────────────────────────────────────────────────
    /// Near-black void used for ClearColor and fog
    pub const VOID: Color = Color::oklcha(0.100, 0.0, 0.0, 1.0);
}

/// TODO: text is not working at the moment due to a button ECS hierarchy being tricky
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
        Self {
            none: Palette::new(colors::NEUTRAL300, colors::NEUTRAL900, BorderColor::all(colors::NEUTRAL850)),
            hovered: Palette::new(colors::NEUTRAL300, colors::NEUTRAL850, BorderColor::all(colors::NEUTRAL800)),
            pressed: Palette::new(colors::NEUTRAL300, colors::NEUTRAL800, BorderColor::all(colors::NEUTRAL750)),
            disabled: Palette::new(colors::NEUTRAL500, colors::NEUTRAL900, BorderColor::all(colors::NEUTRAL850)),
        }
    }
}
