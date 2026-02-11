use super::*;

/// This plugin is responsible for the game menu
/// The menu is only drawn during the State [`Screen::Title`] and is removed when that state is exited
pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::Title), setup_menu);
}

fn setup_menu(
    mut commands: Commands,
    mut state: ResMut<Session>,
    #[cfg(not(target_arch = "wasm32"))] server_state: Option<
        Res<crate::networking::local_server::LocalServerState>,
    >,
) {
    commands
        .spawn((
            DespawnOnExit(Screen::Title),
            GlobalZIndex(1),
            ui_root("Title UI"),
            BackgroundColor(colors::NEUTRAL950),
        ))
        .with_children(|root| {
            root.spawn(Node {
                position_type: PositionType::Absolute,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexStart,
                row_gap: Vh(1.5),
                bottom: Vw(5.0),
                left: Vw(5.0),
                ..default()
            })
            .with_children(|buttons| {
                let menu = || {
                    Props::default()
                        .min_width(Vw(30.0))
                        .padding(UiRect::axes(Vw(8.0), Vh(2.0)))
                };

                // Native: Resume existing or start new singleplayer session
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let has_running_server = server_state.as_ref().is_some_and(|s| {
                        matches!(
                            s.as_ref(),
                            crate::networking::local_server::LocalServerState::Ready
                        )
                    });

                    if has_running_server {
                        let half = || Props::default().padding(UiRect::axes(Vw(2.0), Vh(2.0)));
                        let half_slot = || Node {
                            flex_grow: 1.0,
                            flex_basis: Percent(0.0),
                            flex_direction: FlexDirection::Column,
                            ..default()
                        };
                        buttons.spawn((
                            Node {
                                min_width: Vw(30.0),
                                column_gap: Vh(1.5),
                                ..default()
                            },
                            children![
                                (
                                    half_slot(),
                                    children![btn(half().text("Resume"), to::singleplayer)]
                                ),
                                (
                                    half_slot(),
                                    children![btn(half().text("New Game"), to::new_singleplayer)]
                                ),
                            ],
                        ));
                    } else {
                        buttons.spawn(btn(menu().text("Singleplayer"), to::singleplayer));
                    }
                }

                // Web: "Solo" creates a private session on the remote server
                #[cfg(target_arch = "wasm32")]
                buttons.spawn(btn(menu().text("Solo"), to::solo));

                buttons.spawn(btn(menu().text("Multiplayer"), to::multiplayer));

                buttons.spawn(btn(menu().text("Settings"), to::settings));

                #[cfg(not(target_arch = "wasm32"))]
                buttons.spawn(btn(menu().text("Exit"), exit_app));
            });
        });

    state.reset();
}

#[cfg(not(target_arch = "wasm32"))]
fn exit_app(_: On<Pointer<Click>>, mut app_exit: MessageWriter<AppExit>) {
    app_exit.write(AppExit::Success);
}
