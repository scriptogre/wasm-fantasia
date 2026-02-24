use super::*;
use crate::models::player::FootContact;
use crate::player::control::{Footstep, JumpLaunched, LandingImpact, Sprinting};
use bevy_seedling::prelude::*;

pub fn plugin(app: &mut App) {
    app.add_observer(on_foot_contact)
        .add_observer(jump_sound)
        .add_observer(launch_boom)
        .add_observer(landing_boom);
}

/// Fires on any entity whose AnimationPlayer hits a FootContact event in a locomotion clip.
/// Walks up the hierarchy to find the player root, then triggers sound + Footstep event.
///
/// Sound blends between jog and sprint pools based on actual velocity, not the binary
/// Sprinting component. This creates a smooth audio transition as the character accelerates.
fn on_foot_contact(
    on: On<FootContact>,
    cfg: Res<Config>,
    state: Res<Session>,
    settings: Res<Settings>,
    parents: Query<&ChildOf>,
    transforms: Query<&Transform>,
    sprinting: Query<Has<Sprinting>>,
    controllers: Query<&TnuaController<ControlScheme>>,
    players: Query<(), With<Player>>,
    remote_players: Query<(), With<RemotePlayer>>,
    mut cmds: Commands,
    mut sources: ResMut<AudioSources>,
) {
    if state.muted {
        return;
    }

    // Walk up parent chain from AnimationPlayer entity to find the player root
    let mut entity = on.trigger().target;
    let player_root = loop {
        if players.contains(entity) || remote_players.contains(entity) {
            break entity;
        }
        if let Ok(parent) = parents.get(entity) {
            entity = parent.parent();
        } else {
            return; // no player found in hierarchy
        }
    };

    let Ok(transform) = transforms.get(player_root) else {
        return;
    };

    let is_sprinting = sprinting.get(player_root).unwrap_or(false);
    let mut rng = rand::rng();

    // Velocity-based sound layering: jog step always plays for surface contact,
    // sprint impact layer fades in on top as the character accelerates.
    let sprint_threshold = cfg.player.movement.speed * 1.5;
    let sprint_speed = cfg.player.movement.speed * cfg.player.movement.sprint_factor;

    let actual_speed = controllers
        .get(player_root)
        .map(|c| c.basis_memory.running_velocity.length())
        .unwrap_or(0.0);

    // 0.0 = no sprint layer, 1.0 = full sprint layer volume
    let sprint_blend =
        ((actual_speed - sprint_threshold) / (sprint_speed - sprint_threshold)).clamp(0.0, 1.0);

    // Always play jog step for consistent surface texture
    let jog_handle = sources.jog_steps.pick(&mut rng);
    cmds.spawn(SamplePlayer::new(jog_handle.clone()).with_volume(settings.sfx()));

    // Layer sprint impact on top, volume scales with speed
    if sprint_blend > 0.05 {
        let Volume::Linear(base_vol) = settings.sfx() else {
            cmds.trigger(Footstep {
                position: transform.translation,
                is_sprinting,
            });
            return;
        };
        let sprint_handle = sources.sprint_steps.pick(&mut rng);
        let sprint_vol = Volume::Linear(base_vol * sprint_blend);
        cmds.spawn(SamplePlayer::new(sprint_handle.clone()).with_volume(sprint_vol));
    }

    cmds.trigger(Footstep {
        position: transform.translation,
        is_sprinting,
    });
}

fn jump_sound(
    _: On<Start<Jump>>,
    state: Res<Session>,
    settings: Res<Settings>,
    mut cmds: Commands,
    mut sources: ResMut<AudioSources>,
) -> Result {
    if state.muted {
        return Ok(());
    }

    let mut rng = rand::rng();
    let handle = sources.steps.pick(&mut rng);
    cmds.spawn(SamplePlayer::new(handle.clone()).with_volume(settings.sfx()));

    Ok(())
}

fn launch_boom(
    _on: On<JumpLaunched>,
    state: Res<Session>,
    settings: Res<Settings>,
    mut cmds: Commands,
    mut sources: ResMut<AudioSources>,
) {
    if state.muted {
        return;
    }

    let t = 0.0_f32; // charge removed — always minimum
    let Volume::Linear(base_vol) = settings.sfx() else {
        return;
    };
    let vol_scale = 0.6;
    let volume = Volume::Linear(base_vol * vol_scale * 1.5); // Boost 1.5x for impact feel

    let mut rng = rand::rng();
    let handle = sources.steps.pick(&mut rng);

    // Pitched down (-40% to -60%) — step sample becomes a concussive boom
    let pitch_shift = 0.4 + 0.2 * t; // deeper pitch for bigger charges
    cmds.spawn((
        SamplePlayer::new(handle.clone()).with_volume(volume),
        RandomPitch::new(pitch_shift as f64),
    ));
}

fn landing_boom(
    on: On<LandingImpact>,
    state: Res<Session>,
    settings: Res<Settings>,
    mut cmds: Commands,
    mut sources: ResMut<AudioSources>,
) {
    if state.muted {
        return;
    }

    let event = on.event();
    let t = ((event.velocity_y - 5.0) / 20.0).clamp(0.0, 1.0);

    let Volume::Linear(base_vol) = settings.sfx() else {
        return;
    };
    let vol_scale = 0.5 + 0.5 * t;
    let volume = Volume::Linear(base_vol * vol_scale * 2.0);

    let mut rng = rand::rng();
    let handle = sources.steps.pick(&mut rng);

    // Pitched down for ground-crash feel
    let pitch_shift = 0.3 + 0.2 * t;
    cmds.spawn((
        SamplePlayer::new(handle.clone()).with_volume(volume),
        RandomPitch::new(pitch_shift as f64),
    ));
}
