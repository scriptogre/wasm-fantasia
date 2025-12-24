use super::*;
use bevy::time::common_conditions::on_timer;
use std::time::Duration;

const FADE_TIME: u64 = 2;

pub fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        crossfade_music.run_if(on_timer(Duration::from_secs(FADE_TIME))),
    );

    // app.add_systems(Update, check_fade_completion)
    //     .add_observer(fade_in)
    //     .add_observer(fade_out);
}

markers!(FadeIn, FadeOut);

// fn fade_in(
//     on: On<Add, FadeIn>,
//     settings: Res<Settings>,
//     fade_in: Query<&SampleEffects, With<FadeIn>>,
//     mut volume_nodes: Query<(&VolumeNode, &mut AudioEvents), With<MainBus>>,
// ) {
//     info!("fade_in: {}", on.entity);
//     if let Ok(effects) = fade_in.get(on.entity) {
//         info!("fade_in: {}, effects: {effects:?}", on.entity);
//         if let Ok((node, mut events)) = volume_nodes.get_effect_mut(effects) {
//             info!("fade to music: {}", settings.music().linear());
//             node.fade_to(settings.music(), DurationSeconds(FADE_TIME), &mut events);
//         }
//     }
// }
//
// fn fade_out(
//     on: On<Add, FadeOut>,
//     fade_out: Query<&SampleEffects>,
//     mut volume_nodes: Query<(&VolumeNode, &mut AudioEvents), With<MainBus>>,
// ) -> Result {
//     let effects = fade_out.get(on.entity)?;
//     info!("fade_out: {}", on.entity);
//     if let Ok((node, mut events)) = volume_nodes.get_effect_mut(effects) {
//         info!("fade to silent");
//         node.fade_to(Volume::SILENT, DurationSeconds(FADE_TIME), &mut events);
//     }
//
//     Ok(())
// }

fn crossfade_music(
    settings: Res<Settings>,
    mut fade_out: Query<(Entity, &SampleEffects), (With<FadeOut>, Without<FadeIn>)>,
    mut fade_in: Query<(Entity, &SampleEffects), (With<FadeIn>, Without<FadeOut>)>,
    mut volume_nodes: Query<(&VolumeNode, &mut AudioEvents)>,
    mut commands: Commands,
) {
    let fade_duration = DurationSeconds(FADE_TIME as f64);

    for (e, effects) in fade_out.iter_mut() {
        if let Ok((node, mut events)) = volume_nodes.get_effect_mut(effects) {
            info!("to fade out: {e}");
            let mut audio = commands.entity(e);
            if node.volume.linear() <= 0.01 {
                info!("despawning audio entity: {e}");
                audio.despawn();
            }
            // to prevent doing both fades it makes more sense to
            // remove the FadeIn to not cause a cacophony of sounds
            audio.remove::<FadeIn>();
            info!("fade to silent, entity: {e}");
            node.fade_to(Volume::SILENT, fade_duration, &mut events);
        }
    }

    for (e, effects) in fade_in.iter_mut() {
        if let Ok((node, mut events)) = volume_nodes.get_effect_mut(effects) {
            info!("to fade in: {e}");
            if node.volume.linear() < settings.music().linear() {
                info!("fade to music, entity: {e}");
                node.fade_to(settings.music(), fade_duration, &mut events);
            } else {
                commands.entity(e).remove::<FadeIn>();
            }
        }
    }
}

fn check_fade_completion(
    settings: Res<Settings>,
    volume_nodes: Query<&VolumeNode>,
    fade_in: Query<Entity, With<FadeIn>>,
    fade_out: Query<Entity, With<FadeOut>>,
    mut commands: Commands,
) {
    for entity in fade_in.iter() {
        let Ok(node) = volume_nodes.get(entity) else {
            info!("fade in volume: no node for FadeIn entity: {entity}");
            continue;
        };
        info!(
            "fade in volume: {}, need to match: >= {:?}",
            node.volume.linear(),
            settings.music().linear()
        );

        if node.volume.linear() > settings.music().linear() {
            info!("on_fade_in remove: {entity}");
            commands.entity(entity).remove::<FadeIn>();
        }
    }
    for entity in fade_out.iter() {
        let Ok(node) = volume_nodes.get(entity) else {
            info!("fade out volume: no node for FadeOut entity: {entity}");
            continue;
        };
        info!(
            "fade out volume: {}, need to match: <= {:?}",
            node.volume.linear(),
            0.01
        );

        if node.volume.linear() <= 0.01 {
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
//     event: On<OnPlayMusicTrack>,
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
