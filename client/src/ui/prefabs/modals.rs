use super::*;

pub fn click_to_menu(
    _: On<Pointer<Click>>,
    mut commands: Commands,
    mut modals: ResMut<Modals>,
) {
    // Don't reset session here — keep game paused during the transition
    // frames so gameplay systems don't tick. setup_menu resets on OnEnter(Title).
    modals.clear();
    commands.trigger(GoTo(Screen::Title));
}
pub fn click_spawn_settings(on: On<Pointer<Click>>, mut commands: Commands) {
    commands.trigger(NewModal {
        entity: on.entity,
        modal: Modal::Settings,
    });
}

pub fn settings_modal() -> impl Bundle {
    (SettingsModal, settings_ui())
}

pub fn menu_modal() -> impl Bundle {
    let opts = Props::new("Settings")
        .width(Vw(15.0))
        .padding(UiRect::axes(Vw(2.0), Vw(0.5)));
    (
        MenuModal,
        ui_root("In game menu"),
        GlobalZIndex(200),
        children![(
            Node {
                border: UiRect::all(Px(1.0)),
                padding: UiRect::all(Vw(10.0)),
                left: Px(0.0),
                bottom: Px(0.0),
                ..default()
            },
            children![
                (
                    Node {
                        position_type: PositionType::Absolute,
                        right: Px(32.0),
                        bottom: Px(32.0),
                        ..Default::default()
                    },
                    children![btn(
                        Props::new("back")
                            .width(Vw(5.0))
                            .margin(UiRect::ZERO)
                            .padding(UiRect::axes(Vw(1.0), Px(6.0)))
                            .border(UiRect::DEFAULT),
                        ui::click_pop_modal
                    )]
                ),
                (
                    Node {
                        row_gap: Percent(20.0),
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::Center,
                        align_content: AlignContent::Center,
                        ..default()
                    },
                    children![
                        btn(opts.clone(), click_spawn_settings),
                        btn(opts.text("Main Menu"), click_to_menu)
                    ]
                )
            ]
        )],
    )
}
