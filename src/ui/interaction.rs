use bevy::window::CursorOptions;

use super::*;

pub(super) fn plugin(app: &mut App) {
    app.add_systems(Update, apply_interaction_palette)
        .add_observer(play_on_hover_sound_effect)
        .add_observer(play_on_click_sound_effect);
}

/// Palette for widget interactions. Add this to an entity that supports
/// [`Interaction`]s, such as a button, to change its [`BackgroundColor`]
/// and [`BorderColor`] based on the current interaction state.
///
/// Struct of pairs (bg_color, border_color)
#[derive(Component, Clone, Debug, Reflect)]
#[reflect(Component)]
pub struct UiInteraction {
    pub none: (Color, Color),
    pub hovered: (Color, Color),
    pub pressed: (Color, Color),
}
impl UiInteraction {
    pub const DEFAULT: Self = Self {
        none: (TRANSPARENT, WHITEISH),
        hovered: (LIGHT_BLUE, WHITEISH),
        pressed: (DIM_BLUE, WHITEISH),
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

#[allow(clippy::type_complexity)]
fn apply_interaction_palette(
    mut palette_query: Query<
        (
            &Interaction,
            &UiInteraction,
            &mut BorderColor,
            &mut BackgroundColor,
        ),
        (Changed<Interaction>, Without<DisabledButton>),
    >,
) {
    for (interaction, palette, mut border_color, mut background) in &mut palette_query {
        let (bg, border) = match interaction {
            Interaction::None => palette.none,
            Interaction::Hovered => palette.hovered,
            Interaction::Pressed => palette.pressed,
        };
        *background = bg.into();
        *border_color = border.into();
    }
}

fn play_on_hover_sound_effect(
    click: On<Pointer<Hovered>>,
    settings: Res<Settings>,
    audio_sources: Option<Res<AudioSources>>,
    cursor_opt: Query<&CursorOptions>,
    interaction_query: Query<(), With<Interaction>>,
    mut commands: Commands,
) {
    let Ok(cursor) = cursor_opt.single() else {
        return;
    };
    if !cursor.visible {
        return;
    }

    if let Some(audio_sources) = audio_sources {
        if interaction_query.contains(click.entity) {
            commands.spawn(
                SamplePlayer::new(audio_sources.btn_hover.clone()).with_volume(settings.sfx()),
            );
        }
    }
}

fn play_on_click_sound_effect(
    click: On<Pointer<Click>>,
    settings: Res<Settings>,
    audio_sources: Option<Res<AudioSources>>,
    cursor_opt: Query<&CursorOptions>,
    interaction_query: Query<(), With<Interaction>>,
    mut commands: Commands,
) {
    let Ok(cursor) = cursor_opt.single() else {
        return;
    };
    if !cursor.visible {
        return;
    }

    if let Some(audio_sources) = audio_sources {
        if interaction_query.contains(click.entity) {
            commands.spawn(
                SamplePlayer::new(audio_sources.btn_hover.clone()).with_volume(settings.sfx()),
            );
        }
    }
}
