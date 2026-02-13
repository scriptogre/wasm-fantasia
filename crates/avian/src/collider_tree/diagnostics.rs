#[cfg(feature = "bevy")]
use bevy::{diagnostic::DiagnosticPath, prelude::ReflectResource};
use core::time::Duration;

#[cfg(feature = "bevy")]
use crate::diagnostics::{PhysicsDiagnostics, impl_diagnostic_paths};

/// Diagnostics for [collider trees](crate::collider_tree).
#[derive(Debug, Default)]
#[cfg_attr(
    feature = "bevy",
    derive(bevy::prelude::Resource, bevy::prelude::Reflect)
)]
#[cfg_attr(feature = "bevy", reflect(Resource, Debug))]
pub struct ColliderTreeDiagnostics {
    /// Time spent optimizing [collider trees](crate::collider_tree).
    pub optimize: Duration,
    /// Time spent updating AABBs and BVH nodes.
    pub update: Duration,
}

#[cfg(feature = "bevy")]
impl PhysicsDiagnostics for ColliderTreeDiagnostics {
    fn timer_paths(&self) -> Vec<(&'static DiagnosticPath, Duration)> {
        vec![(Self::OPTIMIZE, self.optimize), (Self::UPDATE, self.update)]
    }
}

#[cfg(feature = "bevy")]
impl_diagnostic_paths! {
    impl ColliderTreeDiagnostics {
        OPTIMIZE: "avian/collider_tree/optimize",
        UPDATE: "avian/collider_tree/update",
    }
}
