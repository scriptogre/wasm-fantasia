use bevy::input::gamepad::{
    GamepadAxisChangedEvent, GamepadButtonChangedEvent, GamepadConnectionEvent,
};

use super::*;

pub fn plugin(app: &mut App) {
    app.add_plugins(EnhancedInputPlugin)
        .add_input_context::<PlayerCtx>()
        .add_input_context::<ModalCtx>()
        .add_systems(Update, (log_gamepad_events, log_gamepad_status))
        .add_observer(rm_player_ctx)
        .add_observer(add_modal_ctx)
        .add_observer(add_player_ctx)
        .add_observer(log_navigate)
        .add_observer(log_jump)
        .add_observer(log_sprint)
        .add_observer(log_crouch_start)
        .add_observer(log_crouch_end)
        .add_observer(log_attack)
        .add_observer(log_escape)
        .add_observer(log_venom_speak);
}

fn log_gamepad_events(
    mut connections: MessageReader<GamepadConnectionEvent>,
    mut buttons: MessageReader<GamepadButtonChangedEvent>,
    mut axes: MessageReader<GamepadAxisChangedEvent>,
) {
    for event in connections.read() {
        debug!("Gamepad connection: {:?}", event);
    }
    for event in buttons.read() {
        if event.value.abs() > 0.1 {
            trace!("Gamepad button: {:?} = {:.2}", event.button, event.value);
        }
    }
    for event in axes.read() {
        if event.value.abs() > 0.1 {
            trace!("Gamepad axis: {:?} = {:.2}", event.axis, event.value);
        }
    }
}

/// Log gamepad detection status. Runs every frame but only logs on state changes.
fn log_gamepad_status(gamepads: Query<(Entity, &Gamepad)>, mut prev_count: Local<Option<usize>>) {
    let count = gamepads.iter().count();
    let prev = *prev_count;

    if prev.is_none() || prev != Some(count) {
        if count > 0 {
            for (entity, gamepad) in gamepads.iter() {
                debug!(
                    "Gamepad detected: entity={:?} vendor={:?} product={:?}",
                    entity,
                    gamepad.vendor_id(),
                    gamepad.product_id(),
                );
            }
        } else {
            debug!("No gamepads detected");
        }
        *prev_count = Some(count);
    }
}

fn log_navigate(on: On<Fire<Navigate>>, actions: Query<&Action<Navigate>>) {
    if let Ok(action) = actions.get(on.context) {
        let val = **action;
        if val.length() > 0.1 {
            trace!("Navigate: ({:.2}, {:.2})", val.x, val.y);
        }
    }
}

fn log_jump(_on: On<Start<Jump>>) {
    debug!("Jump");
}

fn log_sprint(_on: On<Start<Sprint>>) {
    debug!("Sprint");
}

fn log_crouch_start(_on: On<Start<Crouch>>) {
    debug!("Crouch start");
}

fn log_crouch_end(_on: On<Complete<Crouch>>) {
    debug!("Crouch end");
}

fn log_attack(_on: On<Start<Attack>>) {
    debug!("Attack");
}

fn log_escape(_on: On<Start<Escape>>) {
    debug!("Escape");
}

fn log_venom_speak(_on: On<Start<VenomSpeak>>) {
    debug!("VenomSpeak");
}

markers!(GlobalCtx, PlayerCtx, ModalCtx);

#[derive(InputAction)]
#[action_output(Vec2)]
pub struct Navigate;

#[derive(InputAction)]
#[action_output(Vec2)]
pub struct Pan;

#[derive(InputAction)]
#[action_output(bool)]
pub struct Attack;

#[derive(InputAction)]
#[action_output(bool)]
pub struct Jump;

#[derive(InputAction)]
#[action_output(bool)]
pub struct Dash;

#[derive(InputAction)]
#[action_output(bool)]
pub struct Sprint;

#[derive(InputAction)]
#[action_output(bool)]
pub struct Crouch;

#[derive(InputAction)]
#[action_output(bool)]
pub struct Pause;

#[derive(InputAction)]
#[action_output(bool)]
pub struct Mute;

#[derive(InputAction)]
#[action_output(bool)]
pub struct Escape;

#[derive(InputAction)]
#[action_output(bool)]
pub struct SpawnEnemy;

#[derive(InputAction)]
#[action_output(bool)]
pub struct ClearEnemies;

#[derive(InputAction)]
#[action_output(bool)]
pub struct VenomSpeak;

#[derive(InputAction)]
#[action_output(Vec2)]
struct NavigateModal;

#[derive(Debug, InputAction)]
#[action_output(bool)]
pub struct Select;

#[derive(Debug, InputAction)]
#[action_output(bool)]
pub struct RightTab;

#[derive(Debug, InputAction)]
#[action_output(bool)]
pub struct LeftTab;

pub fn add_player_ctx(add: On<Add, PlayerCtx>, mut commands: Commands) {
    debug!("PlayerCtx added to {:?}", add.entity);
    let mut e = commands.entity(add.entity);

    e.insert(actions!(PlayerCtx[
        (
            Action::<Pan>::new(),
            ActionSettings {
                require_reset: true,
                ..Default::default()
            },
            Bindings::spawn((
                Spawn((Binding::mouse_motion(),Scale::splat(0.1), Negate::all())),
                Axial::right_stick().with((Scale::splat(2.0), Negate::x())) ,
            )),
        ),

        (
            Action::<Navigate>::new(),
            DeadZone::default(),
            Scale::splat(0.3),
            Bindings::spawn(( Cardinal::wasd_keys(), Cardinal::arrows(), Axial::left_stick() )),
        ),
        (
            Action::<Crouch>::new(),
            bindings![KeyCode::ControlLeft, GamepadButton::LeftTrigger2],
        ),
        (
            Action::<Jump>::new(),
            bindings![KeyCode::Space, GamepadButton::South],
        ),
        (
            Action::<Sprint>::new(),
            bindings![KeyCode::ShiftLeft, GamepadButton::LeftTrigger],
        ),
        (
            Action::<Attack>::new(),
            bindings![MouseButton::Left, GamepadButton::North],
        ),

        (
            Action::<Pause>::new(),
            bindings![KeyCode::KeyP],
        ),
        (
            Action::<Mute>::new(),
            bindings![KeyCode::KeyM],
        ),
        (
            Action::<Escape>::new(),
            ActionSettings {
                require_reset: true,
                ..Default::default()
            },
            bindings![KeyCode::Escape, GamepadButton::Start],
        ),
        (
            Action::<SpawnEnemy>::new(),
            bindings![KeyCode::KeyE, GamepadButton::RightThumb],
        ),
        (
            Action::<ClearEnemies>::new(),
            bindings![KeyCode::KeyQ],
        ),
        (
            Action::<VenomSpeak>::new(),
            bindings![KeyCode::KeyT],
        ),
    ]));
}

fn rm_player_ctx(rm: On<Remove, PlayerCtx>, mut commands: Commands) {
    commands
        .entity(rm.entity)
        .despawn_related::<Actions<PlayerCtx>>();
}

fn add_modal_ctx(add: On<Add, ModalCtx>, mut commands: Commands) {
    commands.entity(add.entity).insert((
        ContextPriority::<ModalCtx>::new(1),
        actions!(ModalCtx[
            (
                Action::<NavigateModal>::new(),
                ActionSettings {
                    require_reset: true,
                    ..Default::default()
                },
                Bindings::spawn((
                    Spawn((Binding::mouse_motion(),Scale::splat(0.1), Negate::all())),
                    Axial::right_stick().with((Scale::splat(2.0), Negate::x())) ,
                )),
            ),
        (
            Action::<Select>::new(),
            bindings![KeyCode::Enter, GamepadButton::South],
        ),
        (
            Action::<RightTab>::new(),
            bindings![KeyCode::BracketRight, GamepadButton::RightTrigger],
        ),
        (
            Action::<LeftTab>::new(),
            bindings![KeyCode::BracketLeft, GamepadButton::LeftTrigger],
        ),
        (
            Action::<Escape>::new(),
                ActionSettings {
                    require_reset: true,
                    ..Default::default()
                },
            bindings![KeyCode::Escape, GamepadButton::Select],
        ),
        ]),
    ));
}
