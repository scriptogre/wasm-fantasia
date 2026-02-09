use super::*;

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

#[macro_export]
macro_rules! timers {
  ( $( $name:ident ),* ) => {
        $(
            #[derive(Component, Reflect, Deref, DerefMut, Debug)]
            #[reflect(Component)]
            pub struct $name(pub Timer);
        )*
    };
}
timers!(JumpTimer, StepTimer);

// macro_rules! sliders {
//   ( $( $name:ident ),* ) => {
//         $(
//             #[derive(Component, Reflect, Debug)]
//             #[reflect(Component)]
//             pub struct $name{
//                 pub current: f32
//             }
//         )*
//     };
// }
//
// sliders!(SliderGeneral, SliderMusic, SliderSfx);
