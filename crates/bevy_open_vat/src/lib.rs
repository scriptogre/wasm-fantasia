pub mod asset;
pub(crate) mod compute;
pub mod data;
pub mod material;
pub mod plugin;
pub mod system;

pub mod prelude {
    pub use crate::asset::{OsRemap, RemapInfo, VatAnimationClip};
    pub use crate::data::{ClipId, VatAnimationController, VatComputeInput, VatComputeResources};
    pub use crate::material::OpenVatExtension;
    pub use crate::plugin::OpenVatPlugin;
}
