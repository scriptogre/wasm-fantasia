use super::*;
use bevy_seedling::prelude::*;
use rand::prelude::*;

pub fn plugin(app: &mut App) {
    app.add_observer(movement_sound)
        .add_observer(dash_sound)
        .add_observer(jump_sound);
}

#[allow(clippy::too_many_arguments)]
fn movement_sound(
    on: Trigger<Fired<Navigate>>,
    time: Res<Time>,
    state: Res<GameState>,
    settings: Res<Settings>,
    tnua: Query<&TnuaController, With<Player>>,
    crouch: Single<&Action<Crouch>>,
    mut cmds: Commands,
    mut sources: ResMut<AudioSources>,
    mut step_timer: Query<&mut StepTimer, With<Player>>,
) -> Result {
    if state.muted || state.paused {
        return Ok(());
    }

    let controller = tnua.get(on.target())?;
    let mut step_timer = step_timer.get_mut(on.target())?;

    let Some((_, basis)) = controller.concrete_basis::<TnuaBuiltinWalk>() else {
        return Ok(());
    };

    // WALK SOUND
    if step_timer.tick(time.delta()).just_finished() && basis.standing_on_entity().is_some() {
        let mut rng = rand::thread_rng();
        let crouch = ***crouch;
        let handle = if crouch {
            // TODO: select crouch steps
            sources.steps.pick(&mut rng)
        } else {
            sources.steps.pick(&mut rng)
        };
        cmds.spawn(SamplePlayer::new(handle.clone()).with_volume(settings.sfx()));
    }

    Ok(())
}

fn jump_sound(
    _: Trigger<Started<Jump>>,
    state: Res<GameState>,
    settings: Res<Settings>,
    // jump_timer: Query<&JumpTimer, With<Player>>,
    mut cmds: Commands,
    mut sources: ResMut<AudioSources>,
) -> Result {
    if state.muted || state.paused {
        return Ok(());
    }

    // let jump_timer = jump_timer.get(on.target())?;
    // if jump_timer.just_finished() {
    let mut rng = thread_rng();
    let handle = sources.steps.pick(&mut rng);
    cmds.spawn(SamplePlayer::new(handle.clone()).with_volume(settings.sfx()));
    // }

    Ok(())
}

fn dash_sound(
    _: Trigger<Started<Dash>>,
    state: Res<GameState>,
    settings: Res<Settings>,
    // jump_timer: Query<&JumpTimer, With<Player>>,
    mut cmds: Commands,
    mut sources: ResMut<AudioSources>,
) -> Result {
    if state.muted || state.paused {
        return Ok(());
    }

    // let jump_timer = jump_timer.get(on.target())?;
    // if jump_timer.just_finished() {
    let mut rng = thread_rng();
    let handle = sources.steps.pick(&mut rng);
    cmds.spawn(SamplePlayer::new(handle.clone()).with_volume(settings.sfx()));
    // }

    Ok(())
}
