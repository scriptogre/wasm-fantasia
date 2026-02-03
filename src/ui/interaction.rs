use super::*;
use bevy::window::CursorOptions;
#[cfg(not(target_arch = "wasm32"))]
use bevy_seedling::prelude::*;

// TODO: there is quite a lot of duplication, maybe there is a better way

pub(super) fn plugin(app: &mut App) {
    app.add_observer(on_hover)
        .add_observer(on_click)
        .add_observer(on_out);
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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
    #[cfg(not(target_arch = "wasm32"))] settings: Res<Settings>,
    #[cfg(not(target_arch = "wasm32"))] sources: Option<Res<AudioSources>>,
    cursor_opt: Query<&CursorOptions>,
    #[cfg(not(target_arch = "wasm32"))] commands: Commands,
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

    #[cfg(not(target_arch = "wasm32"))]
    if let Some(sources) = sources {
        play_click_sound(settings, sources, cursor_opt, commands);
    }
}
fn on_hover(
    hover: On<Pointer<Over>>,
    #[cfg(not(target_arch = "wasm32"))] settings: Res<Settings>,
    #[cfg(not(target_arch = "wasm32"))] sources: Option<Res<AudioSources>>,
    cursor_opt: Query<&CursorOptions>,
    #[cfg(not(target_arch = "wasm32"))] commands: Commands,
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

    #[cfg(not(target_arch = "wasm32"))]
    if let Some(sources) = sources {
        play_hover_sound(settings, sources, cursor_opt, commands);
    }
}

fn on_out(
    hover: On<Pointer<Out>>,
    #[cfg(not(target_arch = "wasm32"))] settings: Res<Settings>,
    #[cfg(not(target_arch = "wasm32"))] sources: Option<Res<AudioSources>>,
    cursor_opt: Query<&CursorOptions>,
    #[cfg(not(target_arch = "wasm32"))] commands: Commands,
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

    #[cfg(not(target_arch = "wasm32"))]
    if let Some(sources) = sources {
        play_hover_sound(settings, sources, cursor_opt, commands);
    }
}

// TODO: adding Disabled observer
// fn on_disable(disable: On<Add, Disabled>, mut commands: Commands) {
//      // painting button gray or something
// }

// fn apply_interaction_palette(
//     mut palette_query: Query<(&PaletteSet, &mut BorderColor, &mut BackgroundColor)>,
//     mut over_events: EventReader<Pointer<Over>>,
//     mut out_events: EventReader<Pointer<Out>>,
//     mut down_events: EventReader<Pointer<Down>>,
//     mut up_events: EventReader<Pointer<Up>>,
// ) {
//     for event in out_events.read() {
//         let Ok((palette, mut border, mut bg)) = palette_query.get_mut(event.target) else {
//             continue;
//         };
//         (*bg, *border) = (palette.none.0.into(), palette.none.1.clone());
//     }
//     for event in down_events.read() {
//         let Ok((palette, mut border, mut bg)) = palette_query.get_mut(event.target) else {
//             continue;
//         };
//         (*bg, *border) = (palette.pressed.0.into(), palette.pressed.1.clone());
//     }
//     for event in up_events.read() {
//         let Ok((palette, mut border, mut bg)) = palette_query.get_mut(event.target) else {
//             continue;
//         };
//         (*bg, *border) = (palette.hovered.0.into(), palette.hovered.1.clone());
//     }
// }
//
// fn play_interaction_sound(
//     settings: Res<Settings>,
//     sources: Res<AudioSources>,
//     cursor_opt: Query<&CursorOptions>,
//     mut down: MessageReader<Pointer<Down>>,
//     mut over: EventReader<Pointer<Over>>,
//     mut commands: Commands,
// ) {
//     if let Ok(cursor) = cursor_opt.single() {
//         if !cursor.visible {
//             return;
//         }
//     }
//
//     let source = if !over.is_empty() {
//         over.clear();
//         sources.hover.clone()
//     } else if !down.is_empty() {
//         down.clear();
//         sources.press.clone()
//     } else {
//         return;
//     };
//     commands.spawn(SamplePlayer::new(source).with_volume(settings.sfx()));
// }
