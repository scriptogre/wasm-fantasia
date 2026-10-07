use super::*;

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::Title), setup_menu);
}

pub(super) const GAME_TITLE: &str = "WASM Fantasia";

fn setup_menu(
    mut commands: Commands,
    mut session: ResMut<Session>,
    settings: Res<Settings>,
    #[cfg(not(target_arch = "wasm32"))] server: Option<
        Res<crate::networking::local_server::LocalServerState>,
    >,
) {
    #[cfg(not(target_arch = "wasm32"))]
    let running = server.is_some_and(|server| {
        matches!(
            *server,
            crate::networking::local_server::LocalServerState::Ready
        )
    });
    let play = |title| {
        Props::new(title)
            .height(Px(52.0))
            .palette_set(PaletteSet::selected())
            .font(TextFont {
                font: fonts::SEMIBOLD,
                weight: bevy::text::FontWeight::SEMIBOLD,
                font_size: 18.0,
                ..default()
            })
    };
    commands.spawn((
        DespawnOnExit(Screen::Title),
        GlobalZIndex(1),
        bevy::input_focus::tab_navigation::TabGroup::default(),
        ui_root("Title"),
        BackgroundColor(colors::PAPER),
        children![
            menu_background(),
            (
                Node {
                    width: Percent(88.0),
                    max_width: Px(560.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Px(20.0),
                    ..default()
                },
                children![
                    header(GAME_TITLE, size::DISPLAY_SIZE),
                    label(
                        Props::new("Strike. Build Fury. Keep moving.")
                            .font_size(17.0)
                            .color(colors::INK_SOFT)
                    ),
                    (
                        Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Px(8.0),
                            width: Percent(100.0),
                            max_width: Px(360.0),
                            margin: UiRect::top(Px(20.0)),
                            ..default()
                        },
                        Children::spawn(SpawnWith(move |menu: &mut ChildSpawner| {
                            #[cfg(target_arch = "wasm32")]
                            menu.spawn(btn(play("Play solo"), to::solo));
                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                menu.spawn(btn(
                                    play(if running { "Resume" } else { "Play solo" }),
                                    to::singleplayer,
                                ));
                                if running {
                                    menu.spawn(btn("New game", to::new_singleplayer));
                                }
                            }
                            menu.spawn(btn(
                                Props::new("Multiplayer").height(Px(52.0)),
                                to::multiplayer,
                            ));
                        }))
                    ),
                    (
                        Node {
                            flex_wrap: FlexWrap::Wrap,
                            column_gap: Px(8.0),
                            margin: UiRect::top(Px(12.0)),
                            ..default()
                        },
                        children![
                            btn(
                                Props::new("Runes")
                                    .icon("runes")
                                    .palette_set(PaletteSet::ghost()),
                                |_: On<Activate>, mut commands: Commands| {
                                    commands.trigger(GoTo(Screen::Runes));
                                }
                            ),
                            btn(
                                Props::new("Settings")
                                    .icon("settings")
                                    .palette_set(PaletteSet::ghost()),
                                to::settings
                            ),
                        ]
                    ),
                ]
            )
        ],
    ));
    session.reset();
    session.screen_shake = settings.screen_shake;
    session.diagnostics = settings.diagnostics;
}
