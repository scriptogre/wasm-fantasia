//! Simple setup for a game: main bus, music and sfx channels
//!
//! The `Music` pool, `Sfx` pool, and `DefaultPool` are all routed to the `MainBus` node.
//! Since each pool has a `VolumeNode`, we can control them all individually. And,
//! since they're all routed to the `MainBus`, we can also set the volume of all three
//! at once.
//!
//! You can see this in action in the knob observers: to set the master volume,
//! we adjust the `MainPool` node, and to set the individual volumes, we adjust the
//! pool nodes.
//!
//! # Example
//! ```rust,no_run
//! use bevy_seedling::{
//!     configuration::{MusicPool, SfxBus},
//!     pool::SamplerPool,
//!     prelude::*,
//! };
//! #[derive(Resource, Debug, Clone, Serialize, Deserialize, Reflect)]
//! pub struct Sound {
//!     pub general: f32,
//!     pub music: f32,
//!     pub sfx: f32,
//! }
//!
//! fn lower_general(
//!     mut sound: ResMut<Sound>,
//!     mut general: Single<&mut VolumeNode, With<MainBus>>,
//! ) {
//!     let new_volume = (sound.general - 0.1).max(3.0);
//!     sound.general = new_volume;
//!     general.volume = Volume::Linear(new_volume);
//! }
//!
//! fn play_music(
//!     _: Trigger<Pointer<Click>>,
//!     playing: Query<(), (With<MusicPool>, With<SamplePlayer>)>,
//!     mut commands: Commands,
//!     server: Res<AssetServer>,
//! ) {
//!     // We'll only play music if it's not already playing.
//!     if playing.iter().len() > 0 {
//!         return;
//!     }
//!
//!     commands.spawn((
//!         // Including the `MusicPool` marker queues this sample in the `MusicPool`.
//!         MusicPool,
//!         SamplePlayer::new(source).with_volume(Volume::Decibels(-6.0)),
//!     ));
//! }
//!
//! fn play_sfx(_: Trigger<Pointer<Click>>, mut commands: Commands, server: Res<AssetServer>) {
//!     let source = server.load("caw.ogg");
//!     // The default pool is routed to the `SfxBus`, so we don't
//!     // need to include any special markers for sound effects.
//!     commands.spawn(SamplePlayer::new(source));
//! }
//! ```
//!
use bevy::prelude::*;
use bevy_seedling::prelude::*;

/// Utility for converting a simple `[0.0, 1.0]` range to [`Volume`].
///
///# Example
/// ```
/// use bevy_seedling::prelude::*;
/// use bevy::prelude::*;
///
/// const STEP: f32 = 0.1;
/// const MIN_VOLUME: f32 = 0.0;
/// const MAX_VOLUME: f32 = 1.0;
///
/// pub fn increment_volume(volume: Volume) -> Volume {
///     let perceptual = CONVERTER.volume_to_perceptual(volume);
///     let new_perceptual = (perceptual + STEP).min(MAX_VOLUME);
///     CONVERTER.perceptual_to_volume(new_perceptual)
/// }
/// ```
pub const CONVERTER: PerceptualVolume = PerceptualVolume::new();

pub fn plugin(app: &mut App) {
    // #[cfg(target_arch = "wasm32")]
    // app.add_plugins(
    //     bevy_seedling::SeedlingPlugin::<firewheel_web_audio::WebAudioBackend> {
    //         config: Default::default(),
    //         stream_config: Default::default(),
    //         spawn_default_pool: true,
    //         pool_size: 4..=32,
    //     },
    // );
    //
    // #[cfg(not(target_arch = "wasm32"))]
    app.add_plugins(bevy_seedling::SeedlingPlugin::default());

    app.add_systems(Startup, setup);
}

fn setup(mut master: Single<&mut VolumeNode, With<MainBus>>) {
    master.volume = CONVERTER.perceptual_to_volume(0.7);
}
