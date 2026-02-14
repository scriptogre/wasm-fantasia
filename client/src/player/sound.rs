use super::*;
use crate::player::control::{Footstep, JumpLaunched, LandingImpact};
use bevy_seedling::prelude::*;

pub fn plugin(app: &mut App) {
    app.add_observer(movement_sound)
        .add_observer(jump_sound)
        .add_observer(launch_boom)
        .add_observer(landing_boom);
}

#[allow(clippy::too_many_arguments)]
fn movement_sound(
    on: On<Fire<Navigate>>,
    time: Res<Time>,
    state: Res<Session>,
    settings: Res<Settings>,
    tnua: Query<(&TnuaController<ControlScheme>, &Transform), With<Player>>,
    crouch: Single<&Action<Crouch>>,
    mut cmds: Commands,
    mut sources: ResMut<AudioSources>,
    mut step_timer: Query<&mut StepTimer, With<Player>>,
) -> Result {
    if state.muted {
        return Ok(());
    }

    let (controller, transform) = tnua.get(on.context)?;
    let mut step_timer = step_timer.get_mut(on.context)?;

    // WALK SOUND
    if step_timer.tick(time.delta()).just_finished()
        && controller.basis_memory.standing_on_entity().is_some()
    {
        let mut rng = rand::rng();
        let crouch = ***crouch;
        let handle = if crouch {
            // TODO: select crouch steps
            sources.steps.pick(&mut rng)
        } else {
            sources.steps.pick(&mut rng)
        };
        cmds.spawn(SamplePlayer::new(handle.clone()).with_volume(settings.sfx()));
        cmds.trigger(Footstep {
            position: transform.translation,
        });
    }

    Ok(())
}

fn jump_sound(
    _: On<Start<Jump>>,
    state: Res<Session>,
    settings: Res<Settings>,
    // jump_timer: Query<&JumpTimer, With<Player>>,
    mut cmds: Commands,
    mut sources: ResMut<AudioSources>,
) -> Result {
    if state.muted {
        return Ok(());
    }

    // let jump_timer = jump_timer.get(on.target())?;
    // if jump_timer.just_finished() {
    let mut rng = rand::rng();
    let handle = sources.steps.pick(&mut rng);
    cmds.spawn(SamplePlayer::new(handle.clone()).with_volume(settings.sfx()));
    // }

    Ok(())
}

fn launch_boom(
    on: On<JumpLaunched>,
    state: Res<Session>,
    settings: Res<Settings>,
    mut cmds: Commands,
    mut sources: ResMut<AudioSources>,
) {
    if state.muted {
        return;
    }

    let event = on.event();
    let t = (event.charge_time / crate::player::control::MAX_CHARGE_TIME).clamp(0.0, 1.0);

    // Volume scales with charge: 60% for tap, 100% for full
    let Volume::Linear(base_vol) = settings.sfx() else {
        return;
    };
    let vol_scale = 0.6 + 0.4 * t;
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
    let volume = Volume::Linear(base_vol * vol_scale * 1.3);

    let mut rng = rand::rng();
    let handle = sources.steps.pick(&mut rng);

    // Pitched down for ground-crash feel
    let pitch_shift = 0.3 + 0.2 * t;
    cmds.spawn((
        SamplePlayer::new(handle.clone()).with_volume(volume),
        RandomPitch::new(pitch_shift as f64),
    ));
}
