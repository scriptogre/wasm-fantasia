use super::*;
use bevy_seedling::prelude::Volume;
use serde::Deserialize;
use std::{error::Error, fs};

pub const SETTINGS_PATH: &str = "assets/settings.ron";

pub fn plugin(app: &mut App) {
    let settings = Settings::load();
    app.insert_resource(settings)
        .init_resource::<ActiveTab>()
        .add_systems(OnExit(Screen::Settings), auto_save_settings);
}

fn auto_save_settings(settings: Res<Settings>) {
    if let Err(e) = settings.save() {
        error!("Failed to auto-save settings: {e}");
    }
}

#[derive(Resource, Reflect, Deserialize, Serialize, Debug, Clone)]
#[reflect(Resource)]
pub struct Settings {
    // audio
    pub sound: SoundPreset,
    // video
    pub fov: f32,
    // keybindings
    pub input_map: InputSettings,
}

impl Settings {
    pub fn general(&self) -> Volume {
        Volume::Linear(self.sound.general)
    }
    pub fn music(&self) -> Volume {
        Volume::Linear(self.sound.general * self.sound.music)
    }

    pub fn sfx(&self) -> Volume {
        Volume::Linear(self.sound.general * self.sound.sfx)
    }

    pub fn load() -> Self {
        match fs::read_to_string(SETTINGS_PATH) {
            Ok(content) => match ron::from_str(&content) {
                Ok(settings) => {
                    info!("Loaded settings from '{SETTINGS_PATH}'");
                    settings
                }
                Err(e) => {
                    warn!("Failed to parse '{SETTINGS_PATH}', using defaults: {e}");
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<(), Box<dyn Error>> {
        if let Some(parent) = std::path::Path::new(SETTINGS_PATH).parent() {
            fs::create_dir_all(parent)?;
        }
        let content = ron::ser::to_string_pretty(self, Default::default())?;
        fs::write(SETTINGS_PATH, content)?;
        Ok(())
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sound: SoundPreset::default(),
            fov: 65.0, // wider for horde combat visibility
            input_map: InputSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Reflect, Component)]
#[reflect(Component)]
pub enum UiTab {
    #[default]
    Audio,
    Video,
}

#[derive(Resource, Default)]
pub struct ActiveTab(pub UiTab);
