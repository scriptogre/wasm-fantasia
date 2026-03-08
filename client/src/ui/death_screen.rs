use crate::*;
use bevy::ui::Val::*;

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::GameOver), setup_death_screen)
        .add_observer(on_restart_run);
}

fn setup_death_screen(mut commands: Commands, mut session: ResMut<Session>) {
    session.paused = true;

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
    }

    commands.trigger(GoTo(Screen::Gameplay));
}
