use bevy::prelude::*;

use crate::models::{Config, Player, SceneCamera, Screen};
use crate::player::control::Sprinting;

pub fn plugin(app: &mut App) {
    app.add_systems(Update, dynamic_fov.run_if(in_state(Screen::Gameplay)));
}

fn dynamic_fov(
    time: Res<Time>,
    cfg: Res<Config>,
    player: Query<Has<Sprinting>, With<Player>>,
    mut camera: Query<&mut Projection, With<SceneCamera>>,
) {
    let Ok(mut projection) = camera.single_mut() else {
        return;
    };
    if let Projection::Perspective(perspective) = &mut *projection {
        let sprinting = player.single().unwrap_or(false);
        let target = (cfg.player.fov + if sprinting { 5.0 } else { 0.0 }).to_radians();
        perspective.fov += (target - perspective.fov) * (1.0 - (-10.0 * time.delta_secs()).exp());
    }
}
