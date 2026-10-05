use super::*;
use bevy::ecs::system::SystemParam;
use bevy::picking::pointer::PointerId;
use bevy::window::CursorOptions;
use bevy_seedling::prelude::*;

pub(super) fn plugin(app: &mut App) {
    app.add_observer(|on: On<Pointer<Over>>, mut ui: Restyle| {
        ui.apply(on.event_target(), |p| &p.hovered, Some(|s| &s.hover));
    })
    .add_observer(|on: On<Pointer<Press>>, mut ui: Restyle| {
        ui.apply(on.event_target(), |p| &p.pressed, Some(|s| &s.press));
    })
    .add_observer(|on: On<Pointer<Release>>, mut ui: Restyle| {
        if matches!(on.pointer_id, PointerId::Touch(_)) {
            ui.apply(on.event_target(), |p| &p.none, None);
        } else {
            ui.apply(on.event_target(), |p| &p.hovered, None);
        }
    })
    .add_observer(|on: On<Pointer<Out>>, mut ui: Restyle| {
        ui.apply(on.event_target(), |p| &p.none, None);
    });
}

type Sound = fn(&AudioSources) -> &Handle<AudioSample>;

/// Swaps a button's colors to one [`PaletteSet`] state and plays its sound.
#[derive(SystemParam)]
struct Restyle<'w, 's> {
    buttons: Query<
        'w,
        's,
        (
            &'static PaletteSet,
            &'static mut BorderColor,
            &'static mut BackgroundColor,
            &'static Children,
        ),
    >,
    texts: Query<'w, 's, &'static mut TextColor>,
    cursor: Query<'w, 's, &'static CursorOptions>,
    settings: Res<'w, Settings>,
    sources: Option<Res<'w, AudioSources>>,
    commands: Commands<'w, 's>,
}

impl Restyle<'_, '_> {
    fn apply(&mut self, entity: Entity, state: fn(&PaletteSet) -> &Palette, sound: Option<Sound>) {
        let Ok((palette, mut border, mut bg, children)) = self.buttons.get_mut(entity) else {
            return;
        };
        let state = state(palette);
        (bg.0, *border) = (state.bg, state.border);
        for c in children {
            if let Ok(mut t) = self.texts.get_mut(*c) {
                t.0 = state.text;
            }
        }

        let (Some(sound), Some(sources)) = (sound, &self.sources) else {
            return;
        };
        if self.cursor.single().is_ok_and(|c| c.visible) {
            self.commands
                .spawn(SamplePlayer::new(sound(sources).clone()).with_volume(self.settings.sfx()));
        }
    }
}
