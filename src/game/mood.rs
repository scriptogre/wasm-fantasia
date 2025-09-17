//! An abstraction for changing mood of the game depending on some triggers
use super::*;
// use bevy::ecs::{component::ComponentId, observer::TriggerTargets};
use rand::prelude::*;

const FADE_TIME: f32 = 2.0;

pub fn plugin(app: &mut App) {
    app.add_systems(OnExit(Screen::Gameplay), stop_soundtrack)
        .add_systems(OnEnter(Screen::Gameplay), start_soundtrack)
        .add_systems(
            Update,
            (fade_in_music, fade_out_music, trigger_mood_change).run_if(in_state(Screen::Gameplay)),
        )
        // .add_observer(trigger_mood_change)
        .add_observer(change_mood);
}

// TODO: implement different music states
// TODO: basic track/mood change per zone
// good structure in this example: <https://github.com/bevyengine/bevy/blob/main/examples/audio/soundtrack.rs#L29>
fn start_soundtrack(
    mut cmds: Commands,
    settings: Res<Settings>,
    sources: ResMut<AudioSources>,
    // boombox: Query<Entity, With<Boombox>>,
) {
    let mut rng = thread_rng();
    let handle = sources.explore.choose(&mut rng).unwrap();

    // // Play music from boombox entity
    // cmds
    //     .entity(boombox.single()?)
    //     .insert(music(handle.clone(), settings.music());
    // Or just play music
    cmds.spawn((
        MusicPool,
        SamplePlayer::new(handle.clone())
            .with_volume(settings.music())
            .looping(),
    ));
}

fn stop_soundtrack(
    // boombox: Query<Entity, With<Boombox>>,
    mut bg_music: Query<&mut PlaybackSettings, With<MusicPool>>,
) {
    for mut s in bg_music.iter_mut() {
        info!("pause track:{s:?}");
        s.pause();
    }
}

fn trigger_mood_change(
    collisions: Collisions,
    zones: Query<(Entity, Option<&Combat>, Option<&Exploration>), With<Zone>>,
    mut commands: Commands,
    mut state: ResMut<GameState>,
    mut player: Query<Entity, With<Player>>,
) {
    let Ok(player) = player.single_mut() else {
        return;
    };
    for (e, combat, exploration) in zones.iter() {
        if collisions.contains(player, e) {
            if combat.is_some() && state.current_mood != MoodType::Combat {
                commands
                    .entity(player)
                    .trigger(ChangeMood(MoodType::Combat));
            }
            if exploration.is_some() && state.current_mood != MoodType::Exploration {
                commands
                    .entity(player)
                    .trigger(ChangeMood(MoodType::Exploration));
                state.current_mood = MoodType::Exploration;
            }
            // info!("sensors: player:{player}, zone:{zone}");
        }
    }
}

// Every time the GameState resource changes, this system is run to trigger the song change.
fn change_mood(
    on: Trigger<ChangeMood>,
    settings: Res<Settings>,
    sources: Res<AudioSources>,
    music: Query<Entity, (With<MusicPool>, With<SamplePlayer>)>,
    mut commands: Commands,
    mut state: ResMut<GameState>,
) {
    let mood = &on.0;
    let mut rng = thread_rng();

    // Fade out all currently running tracks
    for track in music.iter() {
        // commands.entity(track).despawn();
        commands.entity(track).insert(FadeOut);
    }

    info!(
        "current mood: {:?}, change mood:{mood:?}",
        state.current_mood
    );

    // Spawn a new music with the appropriate soundtrack based on new mood
    // Volume is set to start at zero and is then increased by the fade_in system.
    match mood {
        MoodType::Exploration => {
            let handle = sources.explore.choose(&mut rng).unwrap();
            commands.spawn((
                MusicPool,
                SamplePlayer::new(handle.clone())
                    .with_volume(settings.music())
                    .looping(),
                FadeIn,
            ));
        }
        MoodType::Combat => {
            let handle = sources.combat.choose(&mut rng).unwrap();
            commands.spawn((
                MusicPool,
                SamplePlayer::new(handle.clone())
                    .with_volume(settings.music())
                    .looping(),
                FadeIn,
            ));
        }
    }
    state.current_mood = mood.clone();
}

/// Fades in the audio of entities that has the FadeIn component.
/// Removes the FadeIn component once full volume is reached.
fn fade_in_music(
    time: Res<Time>,
    mut sample_players: Query<(Entity, &SampleEffects), With<FadeIn>>,
    mut commands: Commands,
    mut music: Query<&mut VolumeNode, With<FadeIn>>,
) -> Result {
    for (entity, effects) in sample_players.iter_mut() {
        let mut effect = music.get_effect_mut(effects)?;
        info!("fade in volume: {}", effect.volume.linear());
        effect.volume += Volume::Linear(time.delta_secs() / FADE_TIME);

        if effect.volume.linear() >= 1.0 {
            effect.volume = Volume::Linear(1.0);
            commands.entity(entity).remove::<FadeIn>();
        }
    }

    Ok(())
}

/// Fades out the audio of entities that has the FadeOut component.
/// Despawns the entities once audio volume reaches zero.
fn fade_out_music(
    time: Res<Time>,
    mut sample_players: Query<(Entity, &SampleEffects), With<FadeIn>>,
    mut commands: Commands,
    mut music: Query<&mut VolumeNode, With<FadeIn>>,
) -> Result {
    for (entity, effects) in sample_players.iter_mut() {
        let mut effect = music.get_effect_mut(effects)?;
        info!("fade out volume: {}", effect.volume.linear());
        effect.volume -= Volume::Linear(time.delta_secs() / FADE_TIME);

        if effect.volume.linear() <= 0.0 {
            commands.entity(entity).despawn();
        }
    }

    Ok(())
}
