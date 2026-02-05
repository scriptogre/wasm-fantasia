//! Audio setup with main bus, music and sfx channels.
//! Works on both native (cpal backend) and web (WebAudio backend).

use crate::*;
use bevy_seedling::prelude::*;
use std::collections::HashMap;

mod fade;

pub use fade::*;

/// Utility for converting a simple `[0.0, 1.0]` range to [`Volume`].
pub const CONVERTER: PerceptualVolume = PerceptualVolume::new();

pub fn plugin(app: &mut App) {
    #[cfg(target_arch = "wasm32")]
    app.add_plugins(SeedlingPlugin::new_web_audio());

    #[cfg(not(target_arch = "wasm32"))]
    app.add_plugins(SeedlingPlugin::default());

    app.init_resource::<MusicPlaybacks>()
        .add_systems(Startup, setup)
        .add_observer(MusicPlaybacks::track_entity)
        .add_observer(MusicPlaybacks::clear_entity_on_finish)
        .add_plugins(fade::plugin);
}

fn setup(mut master: Single<&mut VolumeNode, With<MainBus>>, settings: Res<Settings>) {
    master.volume = CONVERTER.perceptual_to_volume(settings.general().linear());
}

/// Map of entities that are currently playing music for a specific mood
/// Use them to keep track of [`PlaybackSettings`] and play/pause instead of spawning new ones
#[derive(Resource, Reflect, Debug, Clone, Default, Deref, DerefMut)]
#[reflect(Resource)]
pub struct MusicPlaybacks(HashMap<Mood, Entity>);

impl FromIterator<(Mood, Entity)> for MusicPlaybacks {
    fn from_iter<T: IntoIterator<Item = (Mood, Entity)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl MusicPlaybacks {
    fn track_entity(
        on: On<Add, (Mood, SamplePlayer)>,
        mut music_pb: ResMut<MusicPlaybacks>,
        moods: Query<&Mood>,
    ) {
        if let Ok(&mood) = moods.get(on.entity) {
            info!("adding entity for {mood:?} {}", on.entity);
            music_pb.insert(mood, on.entity);
        }
    }

    /// When [`SamplePlayer`] finishes playing, it removes the entity, so we have to remove entity
    /// from the [`MusicPlaybacks`] resource as well
    fn clear_entity_on_finish(on: On<Despawn, SamplePlayer>, mut music_pb: ResMut<MusicPlaybacks>) {
        music_pb.retain(|z, e| {
            if e == &on.entity {
                info!("removing entity for {z:?} zone");
            }
            e != &on.entity
        });
    }
}
