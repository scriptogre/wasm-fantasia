use super::*;

pub fn plugin(app: &mut App) {
    app.add_observer(add_new_modal)
        .add_observer(pop_modal)
        .add_observer(clear_modals);
}

markers!(MenuModal, SettingsModal, ModalBackdrop);

pub fn click_pop_modal(on: On<Pointer<Click>>, mut commands: Commands) {
    commands.entity(on.entity).trigger(PopModal);
}

pub fn add_new_modal(
    on: On<NewModal>,
    screen: Res<State<Screen>>,
    session: Res<Session>,
    mut commands: Commands,
    mut modals: ResMut<Modals>,
) {
    if *screen.get() != Screen::Gameplay {
        return;
    }

    let mut target = commands.entity(on.entity);
    if modals.is_empty() {
        target.insert(ModalCtx);
        if Modal::Main == on.modal {
            if !session.paused {
                commands.trigger(TogglePause);
            }
            commands.trigger(CamCursorToggle);
        }
        // Spawn persistent backdrop behind all modals
        commands.spawn((
            ModalBackdrop,
            DespawnOnExit(Screen::Gameplay),
            ui_root("Modal Backdrop"),
            GlobalZIndex(199),
            BackgroundColor(colors::NEUTRAL950.with_alpha(0.95)),
        ));
    }

    // despawn all previous modal entities to avoid clattering
    commands.entity(on.entity).trigger(ClearModals);
    match on.event().modal {
        Modal::Main => commands.spawn(menu_modal()),
        Modal::Settings => commands.spawn(settings_modal()),
    };

    modals.push(on.event().modal.clone());
}

pub fn pop_modal(
    _pop: On<PopModal>,
    screen: Res<State<Screen>>,
    menu_marker: Query<Entity, With<MenuModal>>,
    settings_marker: Query<Entity, With<SettingsModal>>,
    backdrop: Query<Entity, With<ModalBackdrop>>,
    mut commands: Commands,
    mut modals: ResMut<Modals>,
) {
    if Screen::Gameplay != *screen.get() {
        return;
    }

    // just a precaution
    assert!(!modals.is_empty());

    let popped = modals.pop().expect("failed to pop modal");
    match popped {
        Modal::Main => {
            if let Ok(menu) = menu_marker.single() {
                commands.entity(menu).despawn();
            }
        }
        Modal::Settings => {
            if let Ok(menu) = settings_marker.single() {
                commands.entity(menu).despawn();
            }
        }
    }

    // respawn next in the modal stack
    if let Some(modal) = modals.last() {
        match modal {
            Modal::Main => commands.spawn(menu_modal()),
            Modal::Settings => commands.spawn(settings_modal()),
        };
    }

    if modals.is_empty() {
        if let Ok(bg) = backdrop.single() {
            commands.entity(bg).despawn();
        }
        commands.trigger(TogglePause);
        commands.trigger(CamCursorToggle);
    }
}

pub fn clear_modals(
    _: On<ClearModals>,
    menu_marker: Query<Entity, With<MenuModal>>,
    settings_marker: Query<Entity, With<SettingsModal>>,
    mut commands: Commands,
    mut modals: ResMut<Modals>,
) {
    for m in &modals.as_deref_mut() {
        match m {
            Modal::Main => {
                if let Ok(modal) = menu_marker.single() {
                    commands.entity(modal).despawn();
                }
            }
            Modal::Settings => {
                if let Ok(modal) = settings_marker.single() {
                    commands.entity(modal).despawn();
                }
            }
        }
    }
}

/// Modal stack. kudo for the idea to @skyemakesgames
/// Only relevant in [`Screen::Gameplay`]
#[derive(Reflect, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Modal {
    Main,
    Settings,
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
