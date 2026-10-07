use crate::*;
use bevy::ui::Val::*;
use bevy_third_person_camera::ThirdPersonCamera;

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

    // A paper band across the dimmed scene
    commands.spawn((
        DespawnOnExit(Screen::GameOver),
        // Above the HUD, below modals
        GlobalZIndex(150),
        ui_root("Death Screen"),
        bevy::input_focus::tab_navigation::TabGroup::default(),
        BackgroundColor(colors::VOID.with_alpha(0.6)),
        children![(
            Node {
                width: Percent(90.0),
                max_width: Px(800.0),
                border_radius: BorderRadius::all(Px(3.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Px(12.0),
                padding: UiRect::axes(Vw(5.0), Px(24.0)),
                ..default()
            },
            BackgroundColor(colors::PAPER),
            children![
                header(
                    Props::new("You died").color(colors::ALERT),
                    size::DISPLAY_SIZE
                ),
                rule(),
                (
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Px(8.0),
                        max_width: Px(360.0),
                        margin: UiRect::top(Px(4.0)),
                        ..default()
                    },
                    children![
                        btn(
                            Props::new("Try again").palette_set(PaletteSet::selected()),
                            try_again
                        ),
                        btn("Main menu", click_to_menu),
                    ],
                ),
            ],
        )],
    ));
}

fn try_again(_: On<Activate>, mut commands: Commands) {
    commands.trigger(RestartRun);
}

fn on_restart_run(
    _: On<RestartRun>,
    conn: Option<Res<crate::networking::SpacetimeDbConnection>>,
    mut commands: Commands,
) {
    if let Some(conn) = conn {
        crate::networking::combat::send_restart_run(&conn);
    } else {
        commands.trigger(GoTo(Screen::Connecting));
    }
}
