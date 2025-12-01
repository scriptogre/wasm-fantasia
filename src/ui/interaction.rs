use super::*;
use bevy::{ecs::entity_disabling::Disabled, window::CursorOptions};

pub(super) fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            trigger_on_press,
            apply_interaction_palette,
            play_interaction_sound
                .run_if(resource_exists::<AudioSources>)
                .in_set(PostPhysicsAppSystems::PlaySounds),
        ),
    );
}

/// Palette for widget interactions. Add this to an entity that supports
/// [`Interaction`]s, such as a button, to change its [`BackgroundColor`]
/// and [`BorderColor`] based on the current interaction state.
///
/// Struct of pairs (bg_color, border_color)
#[derive(Component, Clone, Debug, Reflect)]
pub struct UiPalette {
    pub none: (Color, BorderColor),
    pub hovered: (Color, BorderColor),
    pub pressed: (Color, BorderColor),
}
impl UiPalette {
    pub const DEFAULT: Self = Self {
        none: (colors::TRANSPARENT, BorderColor::DEFAULT),
        hovered: (
            colors::BRIGHT_BLUE,
            BorderColor {
                bottom: colors::WHITEISH,
                ..BorderColor::DEFAULT
            },
        ),
        pressed: (colors::DIM_BLUE, BorderColor::DEFAULT),
    };
    // pub fn all(c: Color) -> Self {
    //     Self {
    //         none: (c, c),
    //         hovered: (c, c),
    //         pressed: (c, c),
    //     }
    // }
    // pub fn none(mut self, c: (Color, Color)) -> Self {
    //     self.none = c;
    //     self
    // }
    // pub fn pressed(mut self, c: (Color, Color)) -> Self {
    //     self.pressed = c;
    //     self
    // }
    // pub fn hovered(mut self, c: (Color, Color)) -> Self {
    //     self.hovered = c;
    //     self
    // }
}

/// Event triggered on a UI entity when the [`Interaction`] component on the same entity changes to
/// [`Interaction::Pressed`]. Observe this event to detect e.g. button presses.
#[derive(EntityEvent)]
pub(crate) struct OnPress {
    pub(crate) entity: Entity,
}

fn trigger_on_press(
    interaction_query: Query<(Entity, &Interaction), Changed<Interaction>>,
    mut commands: Commands,
) {
    for (entity, interaction) in &interaction_query {
        if matches!(interaction, Interaction::Pressed) {
            commands.trigger(OnPress { entity });
        }
    }
}

fn apply_interaction_palette(
    mut palette_query: Query<
        (
            &Interaction,
            &UiPalette,
            &mut BorderColor,
            &mut BackgroundColor,
        ),
        (Changed<Interaction>, Without<Disabled>),
    >,
) {
    for (interaction, palette, mut border_color, mut background) in &mut palette_query {
        let (bg, border) = match interaction {
            Interaction::None => palette.none,
            Interaction::Hovered => palette.hovered,
            Interaction::Pressed => palette.pressed,
        };
        *background = bg.into();
        *border_color = border;
    }
}

fn play_interaction_sound(
    settings: Res<Settings>,
    sources: Res<AudioSources>,
    cursor_opt: Query<&CursorOptions>,
    interaction_query: Query<&Interaction, Changed<Interaction>>,
    mut commands: Commands,
) {
    if let Ok(cursor) = cursor_opt.single() {
        if !cursor.visible {
            return;
        }
    }

    for interaction in &interaction_query {
        let source = match interaction {
            Interaction::Hovered => sources.hover.clone(),
            Interaction::Pressed => sources.press.clone(),
            _ => continue,
        };
        commands.spawn(SamplePlayer::new(source).with_volume(settings.sfx()));
    }
}
