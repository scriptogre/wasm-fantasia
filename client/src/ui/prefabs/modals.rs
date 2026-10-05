use super::*;

pub fn click_to_menu(_: On<Pointer<Click>>, mut commands: Commands, mut modals: ResMut<Modals>) {
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

/// Pause menu. Controls help lives here instead of on the HUD.
pub fn menu_modal(touch: bool) -> impl Bundle {
    let controls = if touch {
        "Drag the right side to look\nHold HIT to attack  ·  JUMP then HIT to slam"
    } else {
        "WASD move  ·  Mouse look  ·  Click attack\nSpace jump  ·  Jump then click to slam"
    };
    (
        MenuModal,
        ui_root("In game menu"),
        GlobalZIndex(200),
        children![(
            panel(360.0),
            children![
                header("Paused"),
                btn(
                    Props::new("Resume").palette_set(PaletteSet::primary()),
                    ui::click_pop_modal
                ),
                btn("Settings", click_spawn_settings),
                btn("Main Menu", click_to_menu),
                label(
                    Props::new(controls)
                        .font_size(size::CAPTION_SIZE)
                        .color(colors::NEUTRAL500)
                        .margin(UiRect::top(Px(8.0)))
                ),
            ]
        )],
    )
}
