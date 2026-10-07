use super::*;

pub fn click_to_menu(_: On<Activate>, mut commands: Commands, mut modals: ResMut<Modals>) {
    // Don't reset session here — keep game paused during the transition
    // frames so gameplay systems don't tick. setup_menu resets on OnEnter(Title).
    modals.clear();
    commands.trigger(GoTo(Screen::Title));
}
pub fn click_spawn_settings(on: On<Activate>, mut commands: Commands) {
    commands.trigger(NewModal {
        entity: on.entity,
        modal: Modal::Settings,
    });
}

pub fn settings_modal() -> impl Bundle {
    (SettingsModal, settings_ui())
}

const DESKTOP_CONTROLS: &[(&str, &str)] = &[
    ("Move", "W A S D"),
    ("Look", "Mouse"),
    ("Attack", "Left click"),
    ("Jump", "Space"),
    ("Slam", "Jump, then attack"),
    ("Sprint", "Shift"),
    ("Pause", "Esc"),
];

const TOUCH_CONTROLS: &[(&str, &str)] = &[
    ("Move", "Left stick"),
    ("Look", "Drag the right side"),
    ("Attack", "Hold Attack"),
    ("Jump", "Jump"),
    ("Slam", "Jump, then Attack"),
];

/// Pause menu. Controls help lives here instead of on the HUD.
pub fn menu_modal(touch: bool) -> impl Bundle {
    let controls = if touch {
        TOUCH_CONTROLS
    } else {
        DESKTOP_CONTROLS
    };
    (
        MenuModal,
        GlobalZIndex(200),
        sheet(
            menu_bar("Paused"),
            (
                columns(),
                children![
                    (
                        column(240.0, 360.0),
                        children![
                            btn(
                                Props::new("Resume").palette_set(PaletteSet::selected()),
                                ui::click_pop_modal
                            ),
                            btn(
                                Props::new("Settings").icon("settings"),
                                click_spawn_settings
                            ),
                            btn(
                                Props::new("Main menu").palette_set(PaletteSet::ghost()),
                                click_to_menu
                            ),
                        ],
                    ),
                    info_panel("Controls", controls),
                ],
            ),
            "Return to the fight.",
        ),
    )
}
