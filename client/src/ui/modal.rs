use super::*;
use crate::player::touch::TouchControls;
use bevy_enhanced_input::prelude::Actions;

#[derive(Component)]
struct ModalPage;
markers!(MenuModal, SettingsModal);

pub fn plugin(app: &mut App) {
    app.add_observer(add_new_modal)
        .add_observer(pop_modal)
        .add_observer(clear_modals);
}

pub fn click_pop_modal(on: On<Activate>, mut commands: Commands) {
    commands.entity(on.entity).trigger(PopModal);
}

fn spawn_modal(modal: &Modal, touch: bool, commands: &mut Commands) {
    match modal {
        Modal::Main => {
            commands.spawn((ModalPage, menu_modal(touch)));
        }
        Modal::Settings => {
            commands.spawn((ModalPage, settings_modal()));
        }
        Modal::Runes => {
            commands.spawn((ModalPage, super::runes::runes_ui()));
        }
    }
}

fn add_new_modal(
    on: On<NewModal>,
    screen: Res<State<Screen>>,
    pause: Res<State<PauseState>>,
    touch: Res<TouchControls>,
    pages: Query<Entity, With<ModalPage>>,
    mut commands: Commands,
    mut modals: ResMut<Modals>,
) {
    if *screen.get() != Screen::Gameplay {
        return;
    }
    if modals.is_empty() {
        commands.entity(on.entity).insert(ModalCtx);
        if *pause.get() != PauseState::Paused {
            commands.trigger(TogglePause);
        }
        commands.trigger(CamCursorToggle);
    }
    for page in &pages {
        commands.entity(page).despawn();
    }
    // Top-level tabs replace each other, so Back always returns to the pause menu.
    if modals.last().is_some_and(|modal| *modal != Modal::Main) {
        modals.pop();
    }
    spawn_modal(&on.modal, touch.enabled, &mut commands);
    modals.push(on.modal.clone());
}

fn pop_modal(
    _: On<PopModal>,
    screen: Res<State<Screen>>,
    pages: Query<Entity, With<ModalPage>>,
    contexts: Query<Entity, With<ModalCtx>>,
    touch: Res<TouchControls>,
    mut commands: Commands,
    mut modals: ResMut<Modals>,
) {
    if *screen.get() != Screen::Gameplay || modals.is_empty() {
        return;
    }
    modals.pop();
    for page in &pages {
        commands.entity(page).despawn();
    }
    if let Some(modal) = modals.last() {
        spawn_modal(modal, touch.enabled, &mut commands);
    } else {
        for entity in &contexts {
            commands
                .entity(entity)
                .remove::<ModalCtx>()
                .despawn_related::<Actions<ModalCtx>>();
        }
        commands.trigger(TogglePause);
        commands.trigger(CamCursorToggle);
    }
}

fn clear_modals(_: On<ClearModals>, pages: Query<Entity, With<ModalPage>>, mut commands: Commands) {
    for page in &pages {
        commands.entity(page).despawn();
    }
}

#[derive(Reflect, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Modal {
    Main,
    Settings,
    Runes,
}
#[derive(EntityEvent)]
pub struct NewModal {
    pub entity: Entity,
    pub modal: Modal,
}
#[derive(EntityEvent)]
pub struct PopModal(pub Entity);
#[derive(EntityEvent)]
pub struct ClearModals(pub Entity);
#[derive(Resource, Deref, DerefMut, Debug, Clone)]
pub struct Modals(pub Vec<Modal>);
