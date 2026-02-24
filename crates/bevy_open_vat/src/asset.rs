use bevy::{
    asset::{AssetLoader, LoadContext, io::Reader},
    platform::collections::HashMap,
    prelude::*,
    reflect::TypePath,
    tasks::ConditionalSendFuture,
};
use serde::Deserialize;
use thiserror::Error;

use crate::data::ClipId;

/// Structure representing the position remapping data from the JSON file.
/// Used to decompress the normalized VAT texture values back into world space positions.
#[derive(Debug, Clone, Copy, Deserialize, Reflect)]
pub struct OsRemap {
    #[serde(rename = "Min")]
    pub min: [f32; 3],
    #[serde(rename = "Max")]
    pub max: [f32; 3],
    #[serde(rename = "Frames")]
    pub frames: u32,
}

/// Structure representing an animation clip defined in the JSON file.
#[derive(Debug, Clone, Copy, Deserialize, Reflect)]
pub struct VatAnimationClip {
    #[serde(rename = "startFrame")]
    pub start_frame: u32,
    #[serde(rename = "endFrame")]
    pub end_frame: u32,
    #[serde(rename = "framerate")]
    pub frame_rate: f32,
    pub looping: bool,
}

impl VatAnimationClip {
    pub fn start_time(&self) -> f32 {
        self.start_frame as f32 / self.frame_rate
    }

    pub fn duration(&self) -> Option<f32> {
        if self.frame_rate <= 1.0 {
            return None;
        }

        Some((self.end_frame - self.start_frame) as f32 / self.frame_rate)
    }
}

/// Main asset structure holding remapping info and animation clips.
/// This corresponds to the sidecar JSON file generated alongside the VAT texture.
#[derive(Debug, Clone, Asset, Deserialize, TypePath)]
pub struct RemapInfo {
    #[serde(rename = "os-remap")]
    pub os_remap: OsRemap,
    pub animations: HashMap<String, VatAnimationClip>,
}

impl RemapInfo {
    /// Look up a clip ID by name. Returns None if the name isn't found.
    /// The ID is the sorted index — sorted to ensure deterministic IDs
    /// regardless of JSON key order.
    pub fn clip_id(&self, name: &str) -> Option<ClipId> {
        let mut names: Vec<&String> = self.animations.keys().collect();
        names.sort();
        names.iter().position(|n| n.as_str() == name).map(|i| i as ClipId)
    }

    /// Get clip data by ID.
    pub fn clip(&self, id: ClipId) -> Option<&VatAnimationClip> {
        let mut names: Vec<&String> = self.animations.keys().collect();
        names.sort();
        names.get(id as usize).and_then(|n| self.animations.get(*n))
    }
}

/// Asset loader for `RemapInfo` files (JSON).
#[derive(Default, TypePath)]
pub(crate) struct RemapInfoAssetLoader;

#[derive(Debug, Error)]
pub enum RemapLoaderError {
    #[error("Failed to load asset for the following reason:{0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to decode asset for the following reason:{0}")]
    Json(#[from] serde_json::Error),
}

impl AssetLoader for RemapInfoAssetLoader {
    type Asset = RemapInfo;
    type Settings = ();
    type Error = RemapLoaderError;

    fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        _load_context: &mut LoadContext,
    ) -> impl ConditionalSendFuture<Output = std::result::Result<Self::Asset, Self::Error>> {
        Box::pin(async move {
            // Read the raw bytes from the asset file.
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).await?;

            // Deserialize the JSON bytes into our serializable format.
            let remap_info = serde_json::from_slice::<RemapInfo>(&bytes)?;

            Ok(remap_info)
        })
    }

    fn extensions(&self) -> &[&str] {
        &["json"]
    }
}
