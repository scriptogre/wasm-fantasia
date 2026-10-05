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
    fonts: Res<crate::asset_loading::Fonts>,
) {
    next_pause.set(PauseState::Paused);

    // Unlock cursor so the player can click UI buttons
    if let Ok(mut cam) = cam.single_mut() {
        cam.cursor_lock_active = false;
    }

    commands.spawn((
        DespawnOnExit(Screen::GameOver),
        // Above the HUD, below modals
        GlobalZIndex(150),
        ui_root("Death Screen"),
        BackgroundColor(colors::VOID.with_alpha(0.75)),
        children![
            (
                Text::new("YOU DIED"),
                TextFont {
                    font: fonts.bold.clone(),
                    font_size: size::DISPLAY_SIZE,
                    ..default()
                },
                TextColor(colors::RED),
            ),
            (
                Node {
                    width: Percent(90.0),
                    max_width: Px(320.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Px(12.0),
                    ..default()
                },
                children![
                    btn(
                        Props::new("Try Again").palette_set(PaletteSet::primary()),
                        try_again
                    ),
                    btn("Main Menu", click_to_menu),
                ],
            ),
        ],
    ));
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
    } else {
        commands.trigger(GoTo(Screen::Connecting));
    }
}
