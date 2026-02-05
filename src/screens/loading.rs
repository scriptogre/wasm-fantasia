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

fn spawn_loading_screen(mut commands: Commands) {
    commands.spawn((
        DespawnOnExit(Screen::Loading),
        ui_root("loading screen"),
        children![label("Loading...")],
    ));
}

fn continue_to_menu_screen(mut next_screen: ResMut<NextState<Screen>>) {
    // Skip title screen in dev mode for faster iteration
    #[cfg(feature = "dev_native")]
    next_screen.set(Screen::Gameplay);
    #[cfg(not(feature = "dev_native"))]
    next_screen.set(Screen::Title);
}

fn all_assets_loaded(resource_handles: Res<ResourceHandles>) -> bool {
    resource_handles.is_all_done()
}
