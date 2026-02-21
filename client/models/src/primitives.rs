use bevy::prelude::*;

/// Macro to hide the derive trait boilerplate
#[macro_export]
macro_rules! markers {
  ( $( $name:ident ),* ) => {
        $(
            #[derive(Component, Reflect, Clone, Default)]
            #[reflect(Component)]
            pub struct $name;
        )*
    };
}

markers!(SceneCamera);

#[derive(Component, Reflect, Clone, Default)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct BlocksGameplay;
