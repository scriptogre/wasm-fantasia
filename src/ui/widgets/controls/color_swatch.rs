use super::*;

/// Marker identifying a color swatch.
#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Clone, Default)]
pub struct ColorSwatch;

/// Marker identifying the color swatch foreground, the piece that actually displays the color
/// in front of the alpha pattern. This exists so that users can reach in and change the color
/// dynamically.
#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Clone, Default)]
pub struct ColorSwatchFg;

/// Template function to spawn a color swatch.
///
/// # Arguments
/// * `overrides` - a bundle of components that are merged in with the normal swatch components.
///
/// # Example
/// ```rust,no_run
/// use bevy::prelude::*;
/// use bevy_ui::widgets::controls::*;
///
/// fn update_colors(
///     colors: Res<DemoWidgetStates>,
///     mut sliders: Query<(Entity, &ColorSlider, &mut SliderBaseColor)>,
///     swatches: Query<(&SwatchType, &Children), With<ColorSwatch>>,
///     mut commands: Commands,
/// ) {
///     if colors.is_changed() {
///         for (swatch_type, children) in swatches.iter() {
///             commands
///                 .entity(children[0])
///                 .insert(BackgroundColor(match swatch_type {
///                     SwatchType::Rgb => colors.rgb_color.into(),
///                     SwatchType::Hsl => colors.hsl_color.into(),
///                 }));
///         }
///     }
/// }
/// ```
pub fn color_swatch<B: Bundle>(overrides: B) -> impl Bundle {
    (
        Node {
            height: size::ROW_HEIGHT,
            min_width: size::ROW_HEIGHT,
            ..Default::default()
        },
        ColorSwatch,
        AlphaPattern,
        MaterialNode::<AlphaPatternMaterial>(Handle::default()),
        BorderRadius::all(Val::Px(5.0)),
        overrides,
        children![(
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.),
                top: Val::Px(0.),
                bottom: Val::Px(0.),
                right: Val::Px(0.),
                ..Default::default()
            },
            ColorSwatchFg,
            BackgroundColor(palette::BRIGHT_BLUE.with_alpha(0.5)),
            BorderRadius::all(Val::Px(5.0))
        ),],
    )
}
