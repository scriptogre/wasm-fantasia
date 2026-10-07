//! A loading screen during which game assets are loaded.
//! This reduces stuttering, especially for audio on WASM.

use super::*;

pub(super) fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::Loading), spawn_loading_screen)
        .add_systems(
            Update,
            continue_to_menu_screen.run_if(in_state(Screen::Loading).and(all_assets_loaded)),
        );
}

/// Same layout as the web page loader and the title screen, so the handoff is seamless
fn spawn_loading_screen(mut commands: Commands) {
    commands.spawn((
        DespawnOnExit(Screen::Loading),
        ui_root("Loading"),
        BackgroundColor(colors::PAPER),
        children![
            menu_background(),
            (
                Node {
                    width: Percent(88.0),
                    max_width: Px(560.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Px(32.0),
                    ..default()
                },
                children![
                    header(title::GAME_TITLE, size::DISPLAY_SIZE),
                    label(
                        Props::new("Preparing the arena…")
                            .font_size(15.0)
                            .color(colors::INK_SOFT)
                    )
                ]
            )
        ],
    ));
}

fn continue_to_menu_screen(mut next_screen: ResMut<NextState<Screen>>) {
    next_screen.set(Screen::Title);
}

fn all_assets_loaded(resource_handles: Res<ResourceHandles>) -> bool {
    resource_handles.is_all_done()
}
