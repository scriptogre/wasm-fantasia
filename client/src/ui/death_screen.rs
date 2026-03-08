use crate::*;
use bevy::ui::Val::*;
use bevy_third_person_camera::ThirdPersonCamera;
use spacetimedb_sdk::DbContext;

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::GameOver), setup_death_screen)
        .add_observer(on_restart_run);
}

fn setup_death_screen(
    mut commands: Commands,
    mut next_pause: ResMut<NextState<PauseState>>,
    mut cam: Query<&mut ThirdPersonCamera>,
) {
    next_pause.set(PauseState::Paused);

    // Unlock cursor so the player can click UI buttons
    if let Ok(mut cam) = cam.single_mut() {
        cam.cursor_lock_active = false;
    }

    commands
        .spawn((
            DespawnOnExit(Screen::GameOver),
            GlobalZIndex(10),
            ui_root("Death Screen"),
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
        ))
        .with_children(|root| {
            root.spawn(
                Props::new("You Died")
                    .font_size(64.0)
                    .color(colors::NEUTRAL100)
                    .bg_color(Color::NONE)
                    .border(UiRect::ZERO)
                    .into_text_bundle(),
            );

            root.spawn(btn(
                Props::default()
                    .text("Try Again")
                    .min_width(Vw(20.0))
                    .padding(UiRect::axes(Vw(4.0), Vh(2.0))),
                try_again,
            ));
        });
}

fn try_again(_: On<Pointer<Click>>, mut commands: Commands) {
    commands.trigger(RestartRun);
}

fn on_restart_run(
    _: On<RestartRun>,
    conn: Option<Res<crate::networking::SpacetimeDbConnection>>,
    mut commands: Commands,
) {
    if let Some(conn) = conn {
        crate::networking::combat::send_restart_run(&conn);
        // Force disconnect so the Connecting screen gets a fresh subscription
        // with full state resync. Without this, the server thinks we already
        // know about all entities and won't re-send them.
        let _ = conn.conn.disconnect();
        commands.remove_resource::<crate::networking::SpacetimeDbConnection>();
    }

    // Go through Connecting — keeps ServerTarget and GameMode alive,
    // reconnects with a fresh subscription, then enters Gameplay.
    commands.trigger(GoTo(Screen::Connecting));
}
