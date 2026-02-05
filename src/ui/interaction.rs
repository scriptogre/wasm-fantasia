use super::*;
use bevy::window::CursorOptions;
use bevy_seedling::prelude::*;

// TODO: there is quite a lot of duplication, maybe there is a better way

pub(super) fn plugin(app: &mut App) {
    app.add_observer(on_hover)
        .add_observer(on_click)
        .add_observer(on_out);
}

fn play_click_sound(
    settings: Res<Settings>,
    sources: Res<AudioSources>,
    cursor_opt: Query<&CursorOptions>,
    mut commands: Commands,
) {
    if let Ok(cursor) = cursor_opt.single() {
        if cursor.visible {
            commands.spawn(SamplePlayer::new(sources.press.clone()).with_volume(settings.sfx()));
        }
    }
}

fn play_hover_sound(
    settings: Res<Settings>,
    sources: Res<AudioSources>,
    cursor_opt: Query<&CursorOptions>,
    mut commands: Commands,
) {
    if let Ok(cursor) = cursor_opt.single() {
        if cursor.visible {
            commands.spawn(SamplePlayer::new(sources.hover.clone()).with_volume(settings.sfx()));
        }
    }
}

fn on_click(
    click: On<Pointer<Click>>,
    settings: Res<Settings>,
    sources: Option<Res<AudioSources>>,
    cursor_opt: Query<&CursorOptions>,
    commands: Commands,
    mut palette_q: Query<(
        &PaletteSet,
        &mut BorderColor,
        &mut BackgroundColor,
        &mut Children,
    )>,
    mut text_color_q: Query<&mut TextColor>,
) {
    let Ok((palette, mut border, mut bg, children)) = palette_q.get_mut(click.event_target())
    else {
        return;
    };
    (*bg, *border) = (palette.pressed.bg.into(), palette.pressed.border);

    for c in &*children {
        if let Ok(mut t) = text_color_q.get_mut(*c) {
            t.0 = palette.hovered.text;
        }
    }

    if let Some(sources) = sources {
        play_click_sound(settings, sources, cursor_opt, commands);
    }
}
fn on_hover(
    hover: On<Pointer<Over>>,
    settings: Res<Settings>,
    sources: Option<Res<AudioSources>>,
    cursor_opt: Query<&CursorOptions>,
    commands: Commands,
    mut palette_q: Query<(
        &PaletteSet,
        &mut BorderColor,
        &mut BackgroundColor,
        &mut Children,
    )>,
    mut text_color_q: Query<&mut TextColor>,
) {
    let Ok((palette, mut border, mut bg, children)) = palette_q.get_mut(hover.event_target())
    else {
        return;
    };
    (*bg, *border) = (palette.hovered.bg.into(), palette.hovered.border);

    for c in &*children {
        if let Ok(mut t) = text_color_q.get_mut(*c) {
            t.0 = palette.hovered.text;
        }
    }

    if let Some(sources) = sources {
        play_hover_sound(settings, sources, cursor_opt, commands);
    }
}

fn on_out(
    hover: On<Pointer<Out>>,
    settings: Res<Settings>,
    sources: Option<Res<AudioSources>>,
    cursor_opt: Query<&CursorOptions>,
    commands: Commands,
    mut palette_q: Query<(
        &PaletteSet,
        &mut BorderColor,
        &mut BackgroundColor,
        &mut Children,
    )>,

    mut text_color_q: Query<&mut TextColor>,
) {
    let Ok((palette, mut border, mut bg, children)) = palette_q.get_mut(hover.event_target())
    else {
        return;
    };
    (*bg, *border) = (palette.none.bg.into(), palette.none.border);

    for c in &*children {
        if let Ok(mut t) = text_color_q.get_mut(*c) {
            t.0 = palette.hovered.text;
        }
    }

    if let Some(sources) = sources {
        play_hover_sound(settings, sources, cursor_opt, commands);
    }
}
