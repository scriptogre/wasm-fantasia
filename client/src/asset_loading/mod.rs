use crate::*;
use bevy::{asset::Asset, gltf::GltfLoaderSettings};
use bevy_seedling::sample::AudioSample;
use bevy_shuffle_bag::ShuffleBag;

mod ron;
mod tracking;
pub use ron::*;
pub use tracking::*;

pub fn plugin(app: &mut App) {
    // start asset loading
    app.add_plugins(tracking::plugin)
        .add_plugins(RonAssetPlugin::<Config>::default())
        .load_resource_from_path::<Config>("config.ron")
        .add_plugins(RonAssetPlugin::<CreditsPreset>::default())
        .load_resource_from_path::<CreditsPreset>("credits.ron")
        .load_resource::<Textures>()
        // .load_resource::<Fonts>()
        .load_resource::<Models>()
        .load_resource::<AudioSources>();
}

// #[derive(Asset, Clone, Reflect, Resource)]
// #[reflect(Resource)]
// pub struct Fonts {
//     #[dependency]
//     pub custom: Handle<Font>,
// }
//
// impl FromWorld for Fonts {
//     fn from_world(world: &mut World) -> Self {
//         let assets = world.resource::<AssetServer>();
//         Self {
//             custom: assets.load("fonts/custom.ttf"),
//         }
//     }
// }

#[derive(Asset, Clone, Reflect, Resource)]
#[reflect(Resource)]
pub struct Textures {
    #[dependency]
    pub github: Handle<Image>,
    #[dependency]
    pub pause: Handle<Image>,
    #[dependency]
    pub mute: Handle<Image>,
}

impl FromWorld for Textures {
    fn from_world(world: &mut World) -> Self {
        let assets = world.resource::<AssetServer>();
        Self {
            github: assets.load("textures/github.png"),
            pause: assets.load("textures/pause.png"),
            mute: assets.load("textures/mute.png"),
        }
    }
}

#[derive(Asset, Clone, Reflect, Resource)]
#[reflect(Resource)]
pub struct Models {
    #[dependency]
    pub player: Handle<Gltf>,
    #[dependency]
    pub scene: Handle<Gltf>,
}

// TODO: restore player-web.glb for wasm32 once web model generation is validated
const PLAYER_MODEL: &str = "models/player.glb";

impl FromWorld for Models {
    fn from_world(world: &mut World) -> Self {
        let assets = world.resource::<AssetServer>();
        Self {
            player: assets.load_with_settings(PLAYER_MODEL, |settings: &mut GltfLoaderSettings| {
                settings.use_model_forward_direction = Some(true);
            }),
            scene: assets.load("models/scene.glb"),
        }
    }
}

#[derive(Asset, Clone, Reflect, Resource)]
#[reflect(Resource)]
pub struct AudioSources {
    // SFX
    #[dependency]
    pub hover: Handle<AudioSample>,
    #[dependency]
    pub press: Handle<AudioSample>,
    #[dependency]
    pub steps: ShuffleBag<Handle<AudioSample>>,
    #[dependency]
    pub punches: ShuffleBag<Handle<AudioSample>>,

    // music
    #[dependency]
    pub explore: ShuffleBag<Handle<AudioSample>>,
    #[dependency]
    pub combat: ShuffleBag<Handle<AudioSample>>,
}

impl AudioSources {
    pub const BTN_HOVER: &'static str = "audio/sfx/btn-hover.ogg";
    pub const BTN_PRESS: &'static str = "audio/sfx/btn-press.ogg";

    pub const STEPS: &[&'static str] = &[
        "audio/sfx/step.ogg",
        "audio/sfx/step1.ogg",
        "audio/sfx/step2.ogg",
        "audio/sfx/step3.ogg",
        "audio/sfx/step4.ogg",
    ];
    pub const PUNCHES: &[&'static str] = &["audio/sfx/punch.wav"];
    pub const GAMEPLAY: &'static str = "audio/music/embrace-the-fight.ogg";
}

impl FromWorld for AudioSources {
    fn from_world(world: &mut World) -> Self {
        let mut rng = rand::rng();
        let a = world.resource::<AssetServer>();

        let steps = Self::STEPS.iter().map(|p| a.load(*p)).collect::<Vec<_>>();
        let punches = Self::PUNCHES.iter().map(|p| a.load(*p)).collect::<Vec<_>>();
        let gameplay: Handle<AudioSample> = a.load(Self::GAMEPLAY);

        Self {
            steps: ShuffleBag::try_new(steps, &mut rng).unwrap(),
            punches: ShuffleBag::try_new(punches, &mut rng).unwrap(),
            // Same track for both moods
            combat: ShuffleBag::try_new(vec![gameplay.clone()], &mut rng).unwrap(),
            explore: ShuffleBag::try_new(vec![gameplay], &mut rng).unwrap(),
            hover: a.load(Self::BTN_HOVER),
            press: a.load(Self::BTN_PRESS),
        }
    }
}
