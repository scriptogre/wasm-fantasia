use crate::asset_loading::AudioSources;
use crate::combat::HitEvent;
use crate::models::{GameState, Settings};
use bevy::prelude::*;
use bevy_seedling::prelude::*;
use rand::Rng;

pub fn plugin(app: &mut App) {
    app.add_observer(punch_sound);
}

fn punch_sound(
    _on: On<HitEvent>,
    state: Res<GameState>,
    settings: Res<Settings>,
    mut cmds: Commands,
    mut sources: ResMut<AudioSources>,
) {
    if state.muted || state.paused {
        return;
    }

    let mut rng = rand::rng();
    let handle = sources.punches.pick(&mut rng);

    // Volume variation: ±15%
    let Volume::Linear(base_vol) = settings.sfx() else {
        return;
    };
    let vol_variation = rng.random_range(0.85..1.15);
    let volume = Volume::Linear(base_vol * vol_variation);

    cmds.spawn((
        SamplePlayer::new(handle.clone()).with_volume(volume),
        RandomPitch::new(0.08), // ±8% pitch variation
    ));
}
