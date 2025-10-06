//! Design tokens used by bevy_new_3d_rpg themes.
//!
//! The term "design token" is commonly used in UX design to mean the smallest unit of a theme,
//! similar in concept to a CSS variable. Each token represents an assignment of a color or
//! value to a specific visual aspect of a widget, such as background or border.

use super::theme::ThemeToken;

/// Window background
pub const WINDOW_BG: ThemeToken = ThemeToken::new_static("bevy_new_3d_rpg.window.bg");

/// Focus ring
pub const FOCUS_RING: ThemeToken = ThemeToken::new_static("bevy_new_3d_rpg.focus");

/// Regular text
pub const TEXT_MAIN: ThemeToken = ThemeToken::new_static("bevy_new_3d_rpg.text.main");
/// Dim text
pub const TEXT_DIM: ThemeToken = ThemeToken::new_static("bevy_new_3d_rpg.text.dim");

// Normal buttons

/// Regular button background
pub const BUTTON_BG: ThemeToken = ThemeToken::new_static("bevy_new_3d_rpg.button.bg");
/// Regular button background (hovered)
pub const BUTTON_BG_HOVER: ThemeToken = ThemeToken::new_static("bevy_new_3d_rpg.button.bg.hover");
/// Regular button background (disabled)
pub const BUTTON_BG_DISABLED: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.button.bg.disabled");
/// Regular button background (pressed)
pub const BUTTON_BG_PRESSED: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.button.bg.pressed");
/// Regular button text
pub const BUTTON_TEXT: ThemeToken = ThemeToken::new_static("bevy_new_3d_rpg.button.txt");
/// Regular button text (disabled)
pub const BUTTON_TEXT_DISABLED: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.button.txt.disabled");

// Primary ("default") buttons

/// Primary button background
pub const BUTTON_PRIMARY_BG: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.button.primary.bg");
/// Primary button background (hovered)
pub const BUTTON_PRIMARY_BG_HOVER: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.button.primary.bg.hover");
/// Primary button background (disabled)
pub const BUTTON_PRIMARY_BG_DISABLED: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.button.primary.bg.disabled");
/// Primary button background (pressed)
pub const BUTTON_PRIMARY_BG_PRESSED: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.button.primary.bg.pressed");
/// Primary button text
pub const BUTTON_PRIMARY_TEXT: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.button.primary.txt");
/// Primary button text (disabled)
pub const BUTTON_PRIMARY_TEXT_DISABLED: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.button.primary.txt.disabled");

// Slider

/// Background for slider
pub const SLIDER_BG: ThemeToken = ThemeToken::new_static("bevy_new_3d_rpg.slider.bg");
/// Background for slider moving bar
pub const SLIDER_BAR: ThemeToken = ThemeToken::new_static("bevy_new_3d_rpg.slider.bar");
/// Background for slider moving bar (disabled)
pub const SLIDER_BAR_DISABLED: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.slider.bar.disabled");
/// Background for slider text
pub const SLIDER_TEXT: ThemeToken = ThemeToken::new_static("bevy_new_3d_rpg.slider.text");
/// Background for slider text (disabled)
pub const SLIDER_TEXT_DISABLED: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.slider.text.disabled");

// Checkbox

/// Checkbox background around the checkmark
pub const CHECKBOX_BG: ThemeToken = ThemeToken::new_static("bevy_new_3d_rpg.checkbox.bg");
/// Checkbox border around the checkmark (disabled)
pub const CHECKBOX_BG_DISABLED: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.checkbox.bg.disabled");
/// Checkbox background around the checkmark
pub const CHECKBOX_BG_CHECKED: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.checkbox.bg.checked");
/// Checkbox border around the checkmark (disabled)
pub const CHECKBOX_BG_CHECKED_DISABLED: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.checkbox.bg.checked.disabled");
/// Checkbox border around the checkmark
pub const CHECKBOX_BORDER: ThemeToken = ThemeToken::new_static("bevy_new_3d_rpg.checkbox.border");
/// Checkbox border around the checkmark (hovered)
pub const CHECKBOX_BORDER_HOVER: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.checkbox.border.hover");
/// Checkbox border around the checkmark (disabled)
pub const CHECKBOX_BORDER_DISABLED: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.checkbox.border.disabled");
/// Checkbox check mark
pub const CHECKBOX_MARK: ThemeToken = ThemeToken::new_static("bevy_new_3d_rpg.checkbox.mark");
/// Checkbox check mark (disabled)
pub const CHECKBOX_MARK_DISABLED: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.checkbox.mark.disabled");
/// Checkbox label text
pub const CHECKBOX_TEXT: ThemeToken = ThemeToken::new_static("bevy_new_3d_rpg.checkbox.text");
/// Checkbox label text (disabled)
pub const CHECKBOX_TEXT_DISABLED: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.checkbox.text.disabled");

// Radio button

/// Radio border around the checkmark
pub const RADIO_BORDER: ThemeToken = ThemeToken::new_static("bevy_new_3d_rpg.radio.border");
/// Radio border around the checkmark (hovered)
pub const RADIO_BORDER_HOVER: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.radio.border.hover");
/// Radio border around the checkmark (disabled)
pub const RADIO_BORDER_DISABLED: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.radio.border.disabled");
/// Radio check mark
pub const RADIO_MARK: ThemeToken = ThemeToken::new_static("bevy_new_3d_rpg.radio.mark");
/// Radio check mark (disabled)
pub const RADIO_MARK_DISABLED: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.radio.mark.disabled");
/// Radio label text
pub const RADIO_TEXT: ThemeToken = ThemeToken::new_static("bevy_new_3d_rpg.radio.text");
/// Radio label text (disabled)
pub const RADIO_TEXT_DISABLED: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.radio.text.disabled");

// Toggle Switch

/// Switch background around the checkmark
pub const SWITCH_BG: ThemeToken = ThemeToken::new_static("bevy_new_3d_rpg.switch.bg");
/// Switch border around the checkmark (disabled)
pub const SWITCH_BG_DISABLED: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.switch.bg.disabled");
/// Switch background around the checkmark
pub const SWITCH_BG_CHECKED: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.switch.bg.checked");
/// Switch border around the checkmark (disabled)
pub const SWITCH_BG_CHECKED_DISABLED: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.switch.bg.checked.disabled");
/// Switch border around the checkmark
pub const SWITCH_BORDER: ThemeToken = ThemeToken::new_static("bevy_new_3d_rpg.switch.border");
/// Switch border around the checkmark (hovered)
pub const SWITCH_BORDER_HOVER: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.switch.border.hover");
/// Switch border around the checkmark (disabled)
pub const SWITCH_BORDER_DISABLED: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.switch.border.disabled");
/// Switch slide
pub const SWITCH_SLIDE: ThemeToken = ThemeToken::new_static("bevy_new_3d_rpg.switch.slide");
/// Switch slide (disabled)
pub const SWITCH_SLIDE_DISABLED: ThemeToken =
    ThemeToken::new_static("bevy_new_3d_rpg.switch.slide.disabled");
