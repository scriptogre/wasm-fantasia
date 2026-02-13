use core::time::Duration;

#[cfg(feature = "bevy")]
use bevy::{diagnostic::DiagnosticPath, prelude::ReflectResource};

#[cfg(feature = "bevy")]
use crate::diagnostics::{PhysicsDiagnostics, impl_diagnostic_paths};

/// Diagnostics for spatial queries.
#[derive(Debug, Default)]
#[cfg_attr(
    feature = "bevy",
    derive(bevy::prelude::Resource, bevy::prelude::Reflect)
)]
#[cfg_attr(feature = "bevy", reflect(Resource, Debug))]
pub struct SpatialQueryDiagnostics {
    /// Time spent updating [`RayCaster`](super::RayCaster) hits.
    pub update_ray_casters: Duration,
    /// Time spent updating [`ShapeCaster`](super::ShapeCaster) hits.
    pub update_shape_casters: Duration,
}

#[cfg(feature = "bevy")]
impl PhysicsDiagnostics for SpatialQueryDiagnostics {
    fn timer_paths(&self) -> Vec<(&'static DiagnosticPath, Duration)> {
        vec![
            (Self::UPDATE_RAY_CASTERS, self.update_ray_casters),
            (Self::UPDATE_SHAPE_CASTERS, self.update_shape_casters),
        ]
    }
}

#[cfg(feature = "bevy")]
impl_diagnostic_paths! {
    impl SpatialQueryDiagnostics {
        UPDATE_RAY_CASTERS: "avian/spatial_query/update_ray_casters",
        UPDATE_SHAPE_CASTERS: "avian/spatial_query/update_shape_casters",
    }
}
