use super::*;
use bevy::text::LineHeight;

/// This plugin is responsible for the game menu
/// The menu is only drawn during the State [`Screen::Title`] and is removed when that state is exited
pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::Title), setup_menu);
}

fn setup_menu(
    mut commands: Commands,
    mut state: ResMut<Session>,
    fonts: Res<crate::asset_loading::Fonts>,
    #[cfg(not(target_arch = "wasm32"))] server_state: Option<
        Res<crate::networking::local_server::LocalServerState>,
    >,
) {
    commands
        .spawn((
            DespawnOnExit(Screen::Title),
            GlobalZIndex(1),
            ui_root("Title UI"),
            BackgroundColor(colors::VOID),
        ))
        .with_children(|root| {
            // Thumb-reachable column: bottom-left on desktop, full width on phones
            root.spawn(Node {
                position_type: PositionType::Absolute,
                flex_direction: FlexDirection::Column,
                row_gap: Px(12.0),
                left: Vw(8.0),
                right: Vw(8.0),
                bottom: Vh(10.0),
                max_width: Px(360.0),
                ..default()
            })
            .with_children(|menu| {
                menu.spawn((
                    Node {
                        width: Px(40.0),
                        height: Px(4.0),
                        ..default()
                    },
                    BackgroundColor(colors::AMBER),
                ));
                menu.spawn((
                    Text::new("WASM\nFANTASIA"),
                    TextFont {
                        font: fonts.bold.clone(),
                        font_size: size::DISPLAY_SIZE,
                        ..default()
                    },
                    LineHeight::RelativeToFont(1.0),
                    TextColor(colors::NEUTRAL50),
                    Node {
                        margin: UiRect::bottom(Px(20.0)),
                        ..default()
                    },
                ));

                let primary = || Props::default().palette_set(PaletteSet::primary());

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
                        menu.spawn((
                            Node {
                                display: Display::Grid,
                                grid_template_columns: RepeatedGridTrack::flex(2, 1.0),
                                column_gap: Px(12.0),
                                ..default()
                            },
                            children![
                                btn(primary().text("Resume"), to::singleplayer),
                                btn("New Game", to::new_singleplayer),
                            ],
                        ));
                    } else {
                        menu.spawn(btn(primary().text("Singleplayer"), to::singleplayer));
                    }
                }

                // Web: "Solo" creates a private session on the remote server
                #[cfg(target_arch = "wasm32")]
                menu.spawn(btn(primary().text("Solo"), to::solo));

                menu.spawn(btn("Multiplayer", to::multiplayer));

                menu.spawn(btn("Settings", to::settings));

                #[cfg(not(target_arch = "wasm32"))]
                menu.spawn(btn("Exit", exit_app));
            });
        });

    state.reset();
}

#[cfg(not(target_arch = "wasm32"))]
fn exit_app(_: On<Pointer<Click>>, mut app_exit: MessageWriter<AppExit>) {
    app_exit.write(AppExit::Success);
}
