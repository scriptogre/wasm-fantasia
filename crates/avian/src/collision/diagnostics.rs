#[cfg(feature = "bevy")]
use bevy::{diagnostic::DiagnosticPath, prelude::ReflectResource};
use core::time::Duration;

#[cfg(feature = "bevy")]
use crate::diagnostics::{PhysicsDiagnostics, impl_diagnostic_paths};

/// Diagnostics for collision detection.
#[derive(Debug, Default)]
#[cfg_attr(
    feature = "bevy",
    derive(bevy::prelude::Resource, bevy::prelude::Reflect)
)]
#[cfg_attr(feature = "bevy", reflect(Resource, Debug))]
pub struct CollisionDiagnostics {
    /// Time spent finding potential collision pairs in the [broad phase](crate::collision::broad_phase).
    pub broad_phase: Duration,
    /// Time spent updating contacts in the [narrow phase](crate::collision::narrow_phase).
    pub narrow_phase: Duration,
    /// The number of contacts.
    pub contact_count: u32,
}

#[cfg(feature = "bevy")]
impl PhysicsDiagnostics for CollisionDiagnostics {
    fn timer_paths(&self) -> Vec<(&'static DiagnosticPath, Duration)> {
        vec![
            (Self::BROAD_PHASE, self.broad_phase),
            (Self::NARROW_PHASE, self.narrow_phase),
        ]
    }

    fn counter_paths(&self) -> Vec<(&'static DiagnosticPath, u32)> {
        vec![(Self::CONTACT_COUNT, self.contact_count)]
    }
}

#[cfg(feature = "bevy")]
impl_diagnostic_paths! {
    impl CollisionDiagnostics {
        BROAD_PHASE: "avian/collision/broad_phase",
        NARROW_PHASE: "avian/collision/update_contacts",
        CONTACT_COUNT: "avian/collision/contact_count",
    }
}
