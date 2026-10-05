use bevy::prelude::*;
use bevy_seedling::prelude::Volume;
use serde::{Deserialize, Serialize};
use std::error::Error;

use crate::{InputSettings, Screen, SoundPreset};

pub const SETTINGS_PATH: &str = "client/assets/settings.ron";

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
        match storage::read() {
            Ok(Some(content)) => match ron::from_str(&content) {
                Ok(settings) => {
                    info!("Loaded settings from {SETTINGS_LOCATION}");
                    settings
                }
                Err(e) => {
                    warn!("Failed to parse {SETTINGS_LOCATION}, using defaults: {e}");
                    Self::default()
                }
            },
            Ok(None) => Self::default(),
            Err(e) => {
                warn!("Failed to read {SETTINGS_LOCATION}, using defaults: {e}");
                Self::default()
            }
        }
    }

    pub fn save(&self) -> Result<(), Box<dyn Error>> {
        let content = ron::ser::to_string_pretty(self, Default::default())?;
        storage::write(&content)
    }
}

/// Human-readable description of where settings are persisted, for logs.
#[cfg(not(target_arch = "wasm32"))]
pub const SETTINGS_LOCATION: &str = "file 'client/assets/settings.ron'";
#[cfg(target_arch = "wasm32")]
pub const SETTINGS_LOCATION: &str = "localStorage 'wasm-fantasia.settings'";

#[cfg(not(target_arch = "wasm32"))]
mod storage {
    use super::SETTINGS_PATH;
    use std::{error::Error, fs, io, path::Path};

    pub fn read() -> Result<Option<String>, Box<dyn Error>> {
        match fs::read_to_string(SETTINGS_PATH) {
            Ok(content) => Ok(Some(content)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn write(content: &str) -> Result<(), Box<dyn Error>> {
        if let Some(parent) = Path::new(SETTINGS_PATH).parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(SETTINGS_PATH, content)?;
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
mod storage {
    use std::error::Error;

    const KEY: &str = "wasm-fantasia.settings";

    fn local_storage() -> Result<web_sys::Storage, Box<dyn Error>> {
        let window = web_sys::window().ok_or("no browser window")?;
        window
            .local_storage()
            .map_err(|e| format!("localStorage access denied: {e:?}"))?
            .ok_or_else(|| "localStorage unavailable".into())
    }

    pub fn read() -> Result<Option<String>, Box<dyn Error>> {
        local_storage()?
            .get_item(KEY)
            .map_err(|e| format!("localStorage read failed: {e:?}").into())
    }

    pub fn write(content: &str) -> Result<(), Box<dyn Error>> {
        local_storage()?
            .set_item(KEY, content)
            .map_err(|e| format!("localStorage write failed (quota or denied): {e:?}").into())
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sound: SoundPreset::default(),
            fov: 75.0,
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
