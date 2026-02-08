use super::*;

/// This plugin is responsible for the game menu
/// The menu is only drawn during the State [`Screen::Title`] and is removed when that state is exited
pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::Title), setup_menu)
        .add_systems(
            Update,
            show_connection_error
                .run_if(in_state(Screen::Title).and(resource_exists::<ConnectionError>)),
        );
}

fn setup_menu(mut commands: Commands, mut state: ResMut<GameState>) {
    commands
        .spawn((
            DespawnOnExit(Screen::Title),
            GlobalZIndex(1),
            ui_root("Title UI"),
            BackgroundColor(colors::NEUTRAL950.with_alpha(0.95)),
        ))
        .with_children(|root| {
            root.spawn(Node {
                position_type: PositionType::Absolute,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexStart,
                row_gap: Vh(2.5),
                bottom: Vw(5.0),
                left: Vw(5.0),
                ..default()
            })
            .with_children(|buttons| {
                buttons.spawn(btn_big("Singleplayer", to::singleplayer));

                #[cfg(feature = "multiplayer")]
                buttons.spawn(btn_big("Multiplayer", to::multiplayer));
                #[cfg(not(feature = "multiplayer"))]
                buttons.spawn(btn_big_disabled("Multiplayer"));

                buttons.spawn(btn_big("Settings", to::settings));

                #[cfg(not(target_arch = "wasm32"))]
                buttons.spawn(btn_big("Exit", exit_app));
            });
        });

    state.reset();
}

fn show_connection_error(mut commands: Commands, error: Option<Res<ConnectionError>>) {
    if error.is_none() {
        return;
    }
    commands.remove_resource::<ConnectionError>();
    commands.spawn((
        DespawnOnExit(Screen::Title),
        GlobalZIndex(2),
        Node {
            position_type: PositionType::Absolute,
            bottom: Vh(4.0),
            left: Vw(1.0),
            ..default()
        },
        Text::new("Could not connect to server"),
        TextColor(colors::HEALTH_RED),
        Pickable::IGNORE,
    ));
}

#[cfg(not(target_arch = "wasm32"))]
fn exit_app(_: On<Pointer<Click>>, mut app_exit: MessageWriter<AppExit>) {
    app_exit.write(AppExit::Success);
}
