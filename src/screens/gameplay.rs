//! The screen state for the main gameplay.

use super::*;

pub(super) fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::Gameplay), spawn_gameplay_ui)
        .add_observer(toggle_mute)
        .add_observer(toggle_pause)
        .add_observer(trigger_menu_toggle_on_esc)
        .add_observer(add_new_modal)
        .add_observer(pop_modal)
        .add_observer(clear_modals);
}

fn spawn_gameplay_ui(mut cmds: Commands, textures: Res<Textures>, _settings: Res<Settings>) {
    // info!("settings on gameplay enter:{settings:?}");
    let opts = Props::default().hidden().width(Vw(5.0)).height(Vw(5.0));
    cmds.spawn((
        DespawnOnExit(Screen::Gameplay),
        GameplayUi,
        ui_root("Gameplay Ui"),
        children![
            // mute/pause icons
            (
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Start,
                    justify_content: JustifyContent::Start,
                    position_type: PositionType::Absolute,
                    top: Px(0.0),
                    left: Vw(47.5),
                    ..Default::default()
                },
                children![
                    (icon(opts.clone().image(textures.pause.clone())), PauseIcon),
                    (icon(opts.clone().image(textures.mute.clone())), MuteIcon),
                ]
            ),
        ],
    ));
}

fn toggle_pause(
    _: On<TogglePause>,
    mut time: ResMut<Time<Virtual>>,
    mut state: ResMut<GameState>,
    mut pause_label: Query<&mut Node, With<PauseIcon>>,
) {
    if let Ok(mut label) = pause_label.single_mut() {
        if time.is_paused() || state.paused {
            time.unpause();
            label.display = Display::None;
        } else {
            time.pause();
            label.display = Display::Flex;
        }
    }

    state.paused = !state.paused;
    info!("paused: {}", state.paused);
}

fn toggle_mute(
    _: On<ToggleMute>,
    settings: ResMut<Settings>,
    mut state: ResMut<GameState>,
    mut label: Query<&mut Node, With<MuteIcon>>,
    mut music: Single<&mut VolumeNode, (With<MusicPool>, Without<SfxBus>)>,
    mut sfx: Single<&mut VolumeNode, (With<SfxBus>, Without<MusicPool>)>,
) {
    if let Ok(mut node) = label.single_mut() {
        if state.muted {
            music.volume = settings.music();
            sfx.volume = settings.sfx();
            node.display = Display::None;
        } else {
            music.volume = Volume::SILENT;
            sfx.volume = Volume::SILENT;
            node.display = Display::Flex;
        }
    }
    state.muted = !state.muted;
    info!("muted: {}", state.muted);
}

// ============================ UI ============================

fn click_to_menu(on: On<Pointer<Click>>, mut commands: Commands, mut state: ResMut<GameState>) {
    commands.entity(on.entity).insert(ModalCtx);
    commands.trigger(GoTo(Screen::Title));
    state.reset();
}
fn click_pop_modal(on: On<Pointer<Click>>, mut commands: Commands) {
    commands.entity(on.entity).trigger(PopModal);
}
fn click_spawn_settings(on: On<Pointer<Click>>, mut commands: Commands) {
    commands.trigger(NewModal {
        entity: on.entity,
        modal: Modal::Settings,
    });
}

fn trigger_menu_toggle_on_esc(
    on: On<Back>,
    mut commands: Commands,
    screen: Res<State<Screen>>,
    state: ResMut<GameState>,
) {
    if *screen.get() != Screen::Gameplay {
        return;
    }

    if state.modals.is_empty() {
        commands.trigger(NewModal {
            entity: on.entity,
            modal: Modal::Main,
        });
    } else {
        commands.entity(on.entity).trigger(PopModal);
    }
}

fn add_new_modal(
    on: On<NewModal>,
    screen: Res<State<Screen>>,
    mut commands: Commands,
    mut state: ResMut<GameState>,
) {
    if *screen.get() != Screen::Gameplay {
        return;
    }

    let mut modal = commands.entity(on.entity);
    if state.modals.is_empty() {
        modal.insert(ModalCtx);
        if Modal::Main == on.modal {
            if !state.paused {
                commands.trigger(TogglePause);
            }
            commands.entity(on.entity).trigger(CamCursorToggle);
        }
    }

    // despawn all previous modal entities to avoid clattering
    commands.entity(on.entity).trigger(ClearModals);
    match on.event().modal {
        Modal::Main => commands.spawn(menu_modal()),
        Modal::Settings => commands.spawn(settings_modal()),
    };

    state.modals.push(on.event().modal.clone());
}

fn pop_modal(
    pop: On<PopModal>,
    screen: Res<State<Screen>>,
    menu_marker: Query<Entity, With<MenuModal>>,
    settings_marker: Query<Entity, With<SettingsModal>>,
    mut commands: Commands,
    mut state: ResMut<GameState>,
) {
    if Screen::Gameplay != *screen.get() {
        return;
    }

    info!("Chat are we popping? {:?}", state.modals);
    // just a precaution
    assert!(!state.modals.is_empty());

    let popped = state.modals.pop().expect("failed to pop modal");
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
    if let Some(modal) = state.modals.last() {
        match modal {
            Modal::Main => commands.spawn(menu_modal()),
            Modal::Settings => commands.spawn(settings_modal()),
        };
    }

    if state.modals.is_empty() {
        info!("PopModal target entity: {}", pop.event_target());
        commands.trigger(TogglePause);
        commands
            .entity(pop.event_target())
            .insert(ModalCtx)
            .trigger(CamCursorToggle);
    }
}

fn clear_modals(
    _: On<ClearModals>,
    state: ResMut<GameState>,
    menu_marker: Query<Entity, With<MenuModal>>,
    settings_marker: Query<Entity, With<SettingsModal>>,
    mut commands: Commands,
) {
    for m in &state.modals {
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

// MODALS

fn settings_modal() -> impl Bundle {
    (
        DespawnOnExit(Screen::Gameplay),
        SettingsModal,
        settings_ui(),
    )
}

fn menu_modal() -> impl Bundle {
    let opts = Props::new("Settings")
        .width(Vw(15.0))
        .padding(UiRect::axes(Vw(2.0), Vw(0.5)));
    (
        DespawnOnExit(Screen::Gameplay),
        MenuModal,
        ui_root("In game menu"),
        children![(
            BorderColor::all(WHITEISH),
            BackgroundColor(TRANSLUCENT),
            Node {
                border: UiRect::all(Px(2.0)),
                padding: UiRect::all(Vw(10.0)),
                left: Px(0.0),
                bottom: Px(0.0),
                ..default()
            },
            children![
                (
                    Node {
                        position_type: PositionType::Absolute,
                        right: Px(0.0),
                        bottom: Px(0.0),
                        ..Default::default()
                    },
                    children![btn_small(
                        Props::new("back").width(Vw(5.0)).border(UiRect::DEFAULT),
                        click_pop_modal
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
