//! An abstraction for changing mood of the game depending on some triggers
use super::*;
use rand::prelude::*;

const FADE_TIME: f64 = 2.0;

pub fn plugin(app: &mut App) {
    app.add_systems(OnExit(Screen::Gameplay), stop_soundtrack)
        .add_systems(OnEnter(Screen::Gameplay), start_soundtrack)
        .add_systems(
            Update,
            (trigger_mood_change, check_fade_completion).run_if(in_state(Screen::Gameplay)),
        )
        .add_observer(crossfade)
        .add_observer(change_mood);
}

fn start_soundtrack(
    settings: Res<Settings>,
    mut commands: Commands,
    mut sources: ResMut<AudioSources>,
    // boombox: Query<Entity, With<Boombox>>,
) {
    let mut rng = thread_rng();
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
                commands
                    .entity(player)
                    .trigger(ChangeMood(MoodType::Combat));
            }
            if exploration.is_some() && state.current_mood != MoodType::Exploration {
                commands
                    .entity(player)
                    .trigger(ChangeMood(MoodType::Exploration));
            }
        }
    }
}

// Every time the current mood in GameState resource changes,
// this system is run to trigger the song change
fn change_mood(
    on: Trigger<ChangeMood>,
    settings: Res<Settings>,
    music: Query<Entity, With<MusicPool>>,
    mut commands: Commands,
    mut state: ResMut<GameState>,
    mut sources: ResMut<AudioSources>,
) {
    let mood = &on.0;
    let mut rng = rand::thread_rng();

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
    // .observe(crossfade);
    state.current_mood = mood.clone();
}

// fn crossfade_is_active(
//     fade_in: Query<(), With<FadeIn>>,
//     fade_out: Query<(), With<FadeOut>>,
// ) -> bool {
//     !fade_in.is_empty() || !fade_out.is_empty()
// }

fn crossfade(
    on: Trigger<OnAdd, FadeIn>,
    settings: Res<Settings>,
    fade_in: Query<&SampleEffects>,
    mut volume_nodes: Query<(&VolumeNode, &mut AudioEvents)>,
) -> Result {
    let fade_duration = DurationSeconds(FADE_TIME);
    info!("1");
    let effects = fade_in.get(on.target())?;
    info!("2, sample effects on: {}", on.target());
    if let Ok((node, mut events)) = volume_nodes.get_effect_mut(effects) {
        info!("3");
        node.fade_to(settings.music(), fade_duration, &mut events);
    } else {
        info!("no volume node on the entity: {}", on.target());
    }

    Ok(())
}

// fn crossfade(
//     _: Trigger<OnAdd, FadeIn>,
//     settings: Res<Settings>,
//     fade_in: Query<&SampleEffects, With<FadeIn>>,
//     mut volume_nodes: Query<(&VolumeNode, &mut AudioEvents)>,
// ) -> Result {
//     let fade_duration = DurationSeconds(FADE_TIME);
//
//     for effects in fade_out.iter() {
//         let (node, mut events) = volume_nodes.get_effect_mut(effects)?;
//         node.fade_to(Volume::SILENT, fade_duration, &mut events);
//     }
//
//     for effects in fade_in.iter() {
//         let (node, mut events) = volume_nodes.get_effect_mut(effects)?;
//         node.fade_to(settings.music(), fade_duration, &mut events);
//     }
//
//     Ok(())
// }

// fn on_fade_in(
//     _: Trigger<OnAdd, FadeIn>,
//     settings: Res<Settings>,
//     fade_in: Query<&SampleEffects, With<FadeIn>>,
//     mut volume_nodes: Query<(&VolumeNode, &mut AudioEvents)>,
// ) -> Result {
//     info!("on_fade_in");
//
//     for effects in fade_in.iter() {
//         info!("on_fade_in in query");
//         let (node, mut events) = volume_nodes.get_effect_mut(effects)?;
//         info!("on_fade_in in query effects");
//         node.fade_to(settings.music(), FADE_TIME, &mut events);
//
//         info!(
//             "fade in volume: {}, need to match: <= {:?}",
//             node.volume.linear(),
//             settings.music().linear()
//         );
//     }
//
//     Ok(())
// }
//
// fn on_fade_out(
//     _: Trigger<OnAdd, FadeOut>,
//     settings: Res<Settings>,
//     fade_out: Query<&SampleEffects, With<FadeOut>>,
//     mut volume_nodes: Query<(&VolumeNode, &mut AudioEvents)>,
// ) -> Result {
//     info!("on_fade_out");
//
//     for effects in fade_out.iter() {
//         let (node, mut events) = volume_nodes.get_effect_mut(effects)?;
//         node.fade_to(Volume::SILENT, FADE_TIME, &mut events);
//
//         info!(
//             "fade in volume: {}, need to match: <= {:?}",
//             node.volume.linear(),
//             settings.music().linear()
//         );
//     }
//     info!("on_fade_out done");
//
//     Ok(())
// }

fn check_fade_completion(
    settings: Res<Settings>,
    volume_nodes: Query<&VolumeNode>,
    fade_in: Query<Entity, With<FadeIn>>,
    fade_out: Query<Entity, With<FadeOut>>,
    mut commands: Commands,
) {
    for entity in fade_in.iter() {
        let Ok(node) = volume_nodes.get(entity) else {
            continue;
        };
        info!(
            "fade in volume: {}, need to match: >= {:?}",
            node.volume.linear(),
            settings.music().linear()
        );

        if node.volume.linear() >= settings.music().linear() {
            info!("on_fade_in remove: {entity}");
            commands.entity(entity).remove::<FadeIn>();
        }
    }
    for entity in fade_out.iter() {
        let Ok(node) = volume_nodes.get(entity) else {
            continue;
        };
        info!(
            "fade out volume: {}, need to match: <= {:?}",
            node.volume.linear(),
            0.001
        );

        if node.volume.linear() <= 0.001 {
            info!("on_fade_out despawn: {entity}");
            commands.entity(entity).despawn();
        }
    }
}

// #[derive(Component, Reflect, Debug)]
// pub struct Music {
//     pub active_player: Entity,
//     pub reserve: Entity,
// }
//
// #[derive(Event, Reflect, Clone, Debug)]
// pub struct OnPlayMusicTrack {
//     pub music_track: GameMusic,
// }
// fn spawn_music(mut commands: Commands, game_assets: Res<GameAssets>) {
//     let song = game_assets.music.get(&GameMusic::Menu).cloned().unwrap();
//     let player_a = commands
//         .spawn((
//             MusicPool,
//             SamplePlayer {
//                 sample: song.clone(),
//                 repeat_mode: RepeatMode::RepeatEndlessly,
//                 ..default()
//             },
//             sample_effects![VolumeNode {
//                 volume: Volume::Decibels(0.0),
//                 ..default()
//             },],
//         ))
//         .id();
//     let player_b = commands
//         .spawn((
//             MusicPool,
//             SamplePlayer {
//                 sample: song,
//                 repeat_mode: RepeatMode::RepeatEndlessly,
//                 ..default()
//             },
//             sample_effects![VolumeNode {
//                 volume: Volume::SILENT,
//                 ..default()
//             },],
//         ))
//         .id();
//
//     commands.spawn((
//         Name::new("Music"),
//         Music {
//             active_player: player_a,
//             reserve: player_b,
//         },
//     ));
// }
//
// pub fn on_play_music_track(
//     event: Trigger<OnPlayMusicTrack>,
//     mut commands: Commands,
//     mut music_player: Single<&mut Music>,
//     sample_effects: Query<&SampleEffects>,
//     time: Res<Time<Audio>>,
//     mut volume_nodes: Query<(&VolumeNode, &mut AudioEvents)>,
//     game_assets: Res<GameAssets>,
// ) {
//     let fade_seconds = 3.0;
//     debug!("on_play_music_track {:?}", event.music_track);
//     let fade_duration = DurationSeconds(fade_seconds);
//     // fade out active
//     let Ok(sample_effect_active) = sample_effects.get(music_player.active_player) else {
//         return;
//     };
//     let (volume, mut events) = volume_nodes.get_effect_mut(sample_effect_active).unwrap();
//     volume.fade_to(Volume::SILENT, fade_duration, &mut events);
//
//     commands.entity(music_player.reserve).despawn();
//     let song = game_assets.music.get(&event.music_track).cloned().unwrap();
//
//     let next_player = commands
//         .spawn((
//             MusicPool,
//             SamplePlayer {
//                 sample: song,
//                 repeat_mode: RepeatMode::RepeatEndlessly,
//                 ..default()
//             },
//             sample_effects![fade_in(
//                 fade_seconds as f32,
//                 &time,
//                 event.music_track.volume()
//             )],
//         ))
//         .id();
//     let active = music_player.active_player;
//     music_player.active_player = next_player;
//     music_player.reserve = active;
// }
