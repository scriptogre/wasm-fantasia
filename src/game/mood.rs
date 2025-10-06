//! An abstraction for changing mood of the game depending on some triggers
use super::*;

pub fn plugin(app: &mut App) {
    app.add_systems(OnExit(Screen::Gameplay), stop_soundtrack)
        .add_systems(OnEnter(Screen::Gameplay), start_soundtrack)
        .add_systems(
            Update,
            trigger_mood_change.run_if(in_state(Screen::Gameplay)),
        )
        .add_observer(change_mood);
}

fn start_soundtrack(
    settings: Res<Settings>,
    mut commands: Commands,
    mut sources: ResMut<AudioSources>,
    // boombox: Query<Entity, With<Boombox>>,
) {
    let mut rng = rand::rng();
    let handle = sources.explore.pick(&mut rng);

    // // Play music from boombox entity
    // cmds
    //     .entity(boombox.single()?)
    //     .insert(music(handle.clone(), settings.music());
    // Or just play music
    commands.spawn((
        MusicPool,
        SamplePlayer::new(handle.clone())
            .with_volume(settings.music())
            .looping(),
        sample_effects![VolumeNode {
            volume: Volume::SILENT,
            ..default()
        }],
        FadeIn,
    ));
    // .observe(crossfade);
}

fn stop_soundtrack(
    // boombox: Query<&mut PlaybackSettings, With<Boombox>>,
    mut bg_music: Query<&mut PlaybackSettings, With<MusicPool>>,
) {
    for mut s in bg_music.iter_mut() {
        s.pause();
    }
}

fn trigger_mood_change(
    collisions: Collisions,
    state: ResMut<GameState>,
    zones: Query<(Entity, Option<&Combat>, Option<&Exploration>), With<Zone>>,
    mut commands: Commands,
    mut player: Query<Entity, With<Player>>,
) {
    let Ok(player) = player.single_mut() else {
        return;
    };
    for (e, combat, exploration) in zones.iter() {
        if collisions.contains(player, e) {
            if combat.is_some() && state.current_mood != MoodType::Combat {
                commands.trigger(ChangeMood {
                    mood: MoodType::Combat,
                    entity: player,
                });
            }
            if exploration.is_some() && state.current_mood != MoodType::Exploration {
                commands.trigger(ChangeMood {
                    mood: MoodType::Exploration,
                    entity: player,
                })
            }
        }
    }
}

// Every time the current mood in GameState resource changes,
// this system is run to trigger the song change
fn change_mood(
    on: On<ChangeMood>,
    settings: Res<Settings>,
    music: Query<Entity, With<MusicPool>>,
    mut commands: Commands,
    mut state: ResMut<GameState>,
    mut sources: ResMut<AudioSources>,
) {
    let mood = &on.mood;
    let mut rng = rand::rng();

    // Fade out all currently running tracks
    for track in music.iter() {
        commands.entity(track).insert(FadeOut);
    }

    // Spawn a new music with the appropriate soundtrack based on new mood
    // Volume is set to start at zero and is then increased by the fade_in system.
    let handle = match mood {
        MoodType::Exploration => sources.explore.pick(&mut rng),
        MoodType::Combat => sources.combat.pick(&mut rng),
    };

    commands.spawn((
        MusicPool,
        SamplePlayer::new(handle.clone())
            .with_volume(settings.music())
            .looping(),
        sample_effects![VolumeNode {
            volume: Volume::SILENT,
            ..default()
        }],
        FadeIn,
    ));
    state.current_mood = mood.clone();
}
