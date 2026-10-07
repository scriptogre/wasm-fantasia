use super::*;
use bevy::{input_focus::tab_navigation::TabIndex, ui_widgets::*, window::PresentMode};
use bevy_seedling::prelude::*;

#[derive(Component, Clone, Copy)]
enum Setting {
    Master,
    Music,
    Effects,
    Fov,
    Shake,
    Diagnostics,
    #[cfg(not(target_arch = "wasm32"))]
    Vsync,
}
#[derive(Component)]
struct SettingsLayout;
#[derive(Component)]
struct Sidebar;
#[derive(Component)]
struct SettingRow;
#[derive(Component)]
struct SettingControl;
#[derive(Component)]
struct TabContent;
#[derive(Component)]
struct SaveButton;
#[derive(Component)]
struct SaveStatus;
#[derive(Component)]
struct ValueLabel(Setting);
#[derive(Component)]
struct SliderFill;
#[derive(Component)]
struct SwitchTrack;
#[derive(Component)]
struct SwitchKnob;
#[derive(Resource, Default)]
struct SavedSettings {
    contents: String,
    failed: bool,
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<SavedSettings>()
        .add_systems(Startup, capture_saved_settings)
        .add_systems(
            Update,
            (
                populate_settings.run_if(
                    resource_changed::<ActiveTab>.or(any_match_filter::<Added<TabContent>>),
                ),
                apply_settings.run_if(resource_changed::<Settings>),
                update_settings_layout.run_if(any_match_filter::<With<SettingsLayout>>),
                update_controls
                    .run_if(resource_changed::<Settings>.or(any_match_filter::<Added<Setting>>)),
                update_save_state.run_if(
                    resource_changed::<Settings>
                        .or(resource_changed::<SavedSettings>)
                        .or(any_match_filter::<Added<SaveButton>>),
                ),
            )
                .chain(),
        );
}

fn capture_saved_settings(settings: Res<Settings>, mut saved: ResMut<SavedSettings>) {
    saved.contents = ron::to_string(&*settings).unwrap();
}

fn save_settings(_: On<Activate>, settings: Res<Settings>, mut saved: ResMut<SavedSettings>) {
    match settings.save() {
        Ok(()) => {
            saved.contents = ron::to_string(&*settings).unwrap();
            saved.failed = false;
        }
        Err(error) => {
            saved.failed = true;
            error!("Could not save settings: {error}");
        }
    }
}

fn apply_settings(
    settings: Res<Settings>,
    mut session: ResMut<Session>,
    mut windows: Query<&mut Window>,
    mut cameras: Query<&mut Projection>,
    mut volumes: Query<(
        &mut VolumeNode,
        Has<MainBus>,
        Has<SamplerPool<MusicPool>>,
        Has<SoundEffectsBus>,
    )>,
) {
    session.screen_shake = settings.screen_shake;
    session.diagnostics = settings.diagnostics;
    for mut window in &mut windows {
        window.present_mode = if settings.vsync {
            PresentMode::AutoVsync
        } else {
            PresentMode::AutoNoVsync
        };
    }
    for mut projection in &mut cameras {
        if let Projection::Perspective(p) = &mut *projection {
            p.fov = settings.fov.to_radians();
        }
    }
    for (mut volume, master, music, sfx) in &mut volumes {
        if master {
            volume.volume = settings.general();
        } else if music {
            volume.volume = settings.music();
        } else if sfx {
            volume.volume = settings.sfx();
        }
    }
}

fn change_slider(
    on: On<ValueChange<f32>>,
    controls: Query<&Setting>,
    mut settings: ResMut<Settings>,
) {
    let Ok(setting) = controls.get(on.source) else {
        return;
    };
    match setting {
        Setting::Master => settings.sound.general = on.value / 100.0,
        Setting::Music => settings.sound.music = on.value / 100.0,
        Setting::Effects => settings.sound.sfx = on.value / 100.0,
        Setting::Fov => settings.fov = on.value,
        _ => (),
    }
}

fn change_switch(
    on: On<ValueChange<bool>>,
    controls: Query<&Setting>,
    mut settings: ResMut<Settings>,
) {
    let Ok(setting) = controls.get(on.source) else {
        return;
    };
    match setting {
        Setting::Shake => settings.screen_shake = on.value,
        Setting::Diagnostics => settings.diagnostics = on.value,
        #[cfg(not(target_arch = "wasm32"))]
        Setting::Vsync => settings.vsync = on.value,
        _ => (),
    }
}

fn read_value(setting: Setting, settings: &Settings) -> f32 {
    match setting {
        Setting::Master => settings.sound.general * 100.0,
        Setting::Music => settings.sound.music * 100.0,
        Setting::Effects => settings.sound.sfx * 100.0,
        Setting::Fov => settings.fov,
        Setting::Shake => f32::from(settings.screen_shake),
        Setting::Diagnostics => f32::from(settings.diagnostics),
        #[cfg(not(target_arch = "wasm32"))]
        Setting::Vsync => f32::from(settings.vsync),
    }
}

fn update_controls(
    settings: Res<Settings>,
    mut controls: Query<(
        Entity,
        &Setting,
        &Name,
        &mut bevy::a11y::AccessibilityNode,
        Option<&SliderRange>,
    )>,
    children: Query<&Children>,
    mut visuals: Query<(
        &mut Node,
        &mut BackgroundColor,
        Has<SliderThumb>,
        Has<SliderFill>,
        Has<SwitchTrack>,
        Has<SwitchKnob>,
    )>,
    mut labels: Query<(&ValueLabel, &mut Text)>,
    mut commands: Commands,
) {
    for (entity, setting, name, mut accessibility, range) in &mut controls {
        accessibility.set_label(name.as_str());
        let value = read_value(*setting, &settings);
        if range.is_some() {
            commands.entity(entity).insert(SliderValue(value));
        } else if value > 0.0 {
            commands.entity(entity).insert(Checked);
        } else {
            commands.entity(entity).remove::<Checked>();
        }
        for descendant in children.iter_descendants(entity) {
            let Ok((mut node, mut background, thumb, fill, track, knob)) =
                visuals.get_mut(descendant)
            else {
                continue;
            };
            if let Some(range) = range {
                let percent = range.thumb_position(value) * 100.0;
                if thumb {
                    node.left = Percent(percent);
                }
                if fill {
                    node.width = Percent(percent);
                }
            } else {
                if track {
                    background.0 = if value > 0.0 {
                        colors::FILL
                    } else {
                        colors::HOVER
                    };
                }
                if knob {
                    node.left = Px(if value > 0.0 { 24.0 } else { 4.0 });
                    background.0 = if value > 0.0 {
                        colors::PAPER
                    } else {
                        colors::INK_SOFT
                    };
                }
            }
        }
    }
    for (label, mut text) in &mut labels {
        let unit = if matches!(label.0, Setting::Fov) {
            "°"
        } else {
            "%"
        };
        let value = format!("{:.0}{unit}", read_value(label.0, &settings));
        if text.0 != value {
            text.0 = value;
        }
    }
}

fn update_save_state(
    settings: Res<Settings>,
    saved: Res<SavedSettings>,
    buttons: Query<Entity, With<SaveButton>>,
    mut status: Query<&mut Text, With<SaveStatus>>,
    mut commands: Commands,
) {
    let dirty = ron::to_string(&*settings).unwrap() != saved.contents;
    for button in &buttons {
        if dirty {
            commands.entity(button).remove::<InteractionDisabled>();
        } else {
            commands.entity(button).insert(InteractionDisabled);
        }
    }
    for mut text in &mut status {
        let value = if saved.failed {
            "Save failed. Try again."
        } else if dirty {
            "Unsaved changes"
        } else {
            "Saved"
        };
        if text.0 != value {
            text.0 = value.into();
        }
    }
}

fn populate_settings(
    tab: Res<ActiveTab>,
    roots: Query<Entity, With<TabContent>>,
    mut commands: Commands,
    mut tabs: Query<(&UiTab, &mut PaletteSet)>,
) {
    for (kind, mut palette) in &mut tabs {
        *palette = if *kind == tab.0 {
            PaletteSet::selected()
        } else {
            PaletteSet::ghost()
        };
    }
    for root in &roots {
        commands
            .entity(root)
            .despawn_children()
            .with_children(|parent| match tab.0 {
                UiTab::Audio => {
                    parent.spawn(build_option(
                        "Master",
                        "100% is the standard level.",
                        Setting::Master,
                        true,
                    ));
                    parent.spawn(build_option(
                        "Music",
                        "Background music.",
                        Setting::Music,
                        true,
                    ));
                    parent.spawn(build_option(
                        "Effects",
                        "Combat, movement and menus.",
                        Setting::Effects,
                        true,
                    ));
                }
                UiTab::Video => {
                    parent.spawn(build_option(
                        "Field of view",
                        "How much of the arena you can see.",
                        Setting::Fov,
                        true,
                    ));
                    parent.spawn(build_option(
                        "Screen shake",
                        "Camera movement on heavy hits.",
                        Setting::Shake,
                        false,
                    ));
                    parent.spawn(build_option(
                        "Diagnostics",
                        "Frame rate and connection details.",
                        Setting::Diagnostics,
                        false,
                    ));
                    #[cfg(not(target_arch = "wasm32"))]
                    parent.spawn(build_option(
                        "Vertical sync",
                        "Match the display refresh rate.",
                        Setting::Vsync,
                        false,
                    ));
                }
            });
    }
}

fn update_settings_layout(
    windows: Query<Ref<Window>>,
    added: Query<(), Or<(Added<SettingsLayout>, Added<SettingRow>)>>,
    mut nodes: Query<(
        &mut Node,
        Has<SettingsLayout>,
        Has<Sidebar>,
        Has<SettingRow>,
        Has<SettingControl>,
    )>,
) {
    let Ok(window) = windows.single() else { return };
    if !window.is_changed() && added.is_empty() {
        return;
    }
    let mobile = window.width() < 760.0;
    for (mut node, layout, sidebar, row, control) in &mut nodes {
        if layout {
            node.flex_direction = if mobile {
                FlexDirection::Column
            } else {
                FlexDirection::Row
            };
        }
        if sidebar {
            node.flex_direction = if mobile {
                FlexDirection::Row
            } else {
                FlexDirection::Column
            };
            node.width = if mobile { Percent(100.0) } else { Px(176.0) };
        }
        if row {
            node.flex_wrap = if mobile {
                FlexWrap::Wrap
            } else {
                FlexWrap::NoWrap
            };
        }
        if control {
            node.width = if mobile { Percent(100.0) } else { Px(300.0) };
        }
    }
}

pub fn settings_ui() -> impl Bundle {
    (
        GlobalZIndex(200),
        sheet(
            menu_bar("Settings"),
            (
                SettingsLayout,
                Node {
                    width: Percent(100.0),
                    max_width: Px(944.0),
                    align_self: AlignSelf::Center,
                    column_gap: Px(64.0),
                    row_gap: Px(28.0),
                    align_items: AlignItems::FlexStart,
                    ..default()
                },
                children![
                    (
                        Sidebar,
                        Node {
                            width: Px(176.0),
                            flex_shrink: 0.0,
                            flex_direction: FlexDirection::Column,
                            row_gap: Px(6.0),
                            column_gap: Px(6.0),
                            ..default()
                        },
                        children![
                            (
                                UiTab::Audio,
                                btn(
                                    Props::new("Audio")
                                        .icon("audio")
                                        .palette_set(PaletteSet::selected()),
                                    |_: On<Activate>, mut tab: ResMut<ActiveTab>| {
                                        tab.0 = UiTab::Audio;
                                    }
                                )
                            ),
                            (
                                UiTab::Video,
                                btn(
                                    Props::new("Video")
                                        .icon("camera")
                                        .palette_set(PaletteSet::ghost()),
                                    |_: On<Activate>, mut tab: ResMut<ActiveTab>| {
                                        tab.0 = UiTab::Video;
                                    }
                                )
                            ),
                        ]
                    ),
                    (
                        Node {
                            flex_direction: FlexDirection::Column,
                            flex_grow: 1.0,
                            width: Percent(100.0),
                            min_width: Px(0.0),
                            row_gap: Px(32.0),
                            ..default()
                        },
                        children![
                            (
                                TabContent,
                                Node {
                                    flex_direction: FlexDirection::Column,
                                    row_gap: Px(24.0),
                                    ..default()
                                }
                            ),
                            rule(),
                            (
                                Node {
                                    justify_content: JustifyContent::End,
                                    align_items: AlignItems::Center,
                                    column_gap: Px(20.0),
                                    ..default()
                                },
                                children![
                                    (
                                        SaveStatus,
                                        label(
                                            Props::new("Saved")
                                                .font_size(13.0)
                                                .color(colors::INK_SOFT)
                                        )
                                    ),
                                    (
                                        SaveButton,
                                        InteractionDisabled,
                                        btn(
                                            Props::new("Save")
                                                .icon("save")
                                                .palette_set(PaletteSet::selected()),
                                            save_settings
                                        )
                                    ),
                                ]
                            ),
                        ]
                    ),
                ],
            ),
            "",
        ),
    )
}

fn build_option(
    title: &'static str,
    hint: &'static str,
    setting: Setting,
    slider: bool,
) -> impl Bundle {
    (
        SettingRow,
        Node {
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            column_gap: Px(24.0),
            row_gap: Px(4.0),
            min_height: Px(64.0),
            ..default()
        },
        children![
            (
                Node {
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0,
                    row_gap: Px(5.0),
                    ..default()
                },
                children![
                    label(title),
                    label(Props::new(hint).font_size(13.0).color(colors::INK_SOFT)),
                ]
            ),
            (
                SettingControl,
                Node {
                    width: Px(300.0),
                    flex_shrink: 0.0,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::End,
                    column_gap: Px(24.0),
                    ..default()
                },
                Children::spawn(SpawnWith(move |parent: &mut ChildSpawner| {
                    if slider {
                        parent.spawn((Name::new(title), build_slider(setting)));
                        parent.spawn((
                            ValueLabel(setting),
                            label(Props::new("").font_size(15.0).justify(Justify::Right).node(
                                Node {
                                    width: Px(52.0),
                                    flex_shrink: 0.0,
                                    ..default()
                                },
                            )),
                        ));
                    } else {
                        parent.spawn((Name::new(title), build_switch(setting)));
                    }
                }))
            ),
        ],
    )
}

fn build_slider(setting: Setting) -> impl Bundle {
    let (min, max, step) = if matches!(setting, Setting::Fov) {
        (20.0, 160.0, 1.0)
    } else {
        (0.0, 300.0, 5.0)
    };
    (
        setting,
        Slider {
            track_click: TrackClick::Snap,
        },
        SliderRange::new(min, max),
        SliderStep(step),
        TabIndex(0),
        Outline::default(),
        Node {
            flex_grow: 1.0,
            height: Px(44.0),
            justify_content: JustifyContent::Center,
            flex_direction: FlexDirection::Column,
            ..default()
        },
        observe(change_slider),
        children![
            (
                Node {
                    height: Px(4.0),
                    width: Percent(100.0),
                    ..default()
                },
                BackgroundColor(colors::LINE),
                Pickable::IGNORE,
                children![(
                    SliderFill,
                    Node {
                        height: Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(colors::INK),
                    Pickable::IGNORE
                )]
            ),
            (
                Node {
                    position_type: PositionType::Absolute,
                    left: Px(0.0),
                    right: Px(12.0),
                    height: Percent(100.0),
                    align_items: AlignItems::Center,
                    ..default()
                },
                Pickable::IGNORE,
                children![(
                    SliderThumb,
                    Node {
                        position_type: PositionType::Absolute,
                        width: Px(12.0),
                        height: Px(24.0),
                        border_radius: BorderRadius::all(Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(colors::INK),
                    Pickable::IGNORE
                )]
            ),
        ],
    )
}

fn build_switch(setting: Setting) -> impl Bundle {
    (
        setting,
        Checkbox,
        TabIndex(0),
        Outline::default(),
        Node {
            width: Px(64.0),
            height: Px(44.0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        observe(change_switch),
        children![(
            SwitchTrack,
            Node {
                width: Px(48.0),
                height: Px(28.0),
                border_radius: BorderRadius::all(Px(11.0)),
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(colors::HOVER),
            Pickable::IGNORE,
            children![(
                SwitchKnob,
                Node {
                    width: Px(20.0),
                    height: Px(20.0),
                    position_type: PositionType::Absolute,
                    left: Px(4.0),
                    border_radius: BorderRadius::all(Px(3.0)),
                    ..default()
                },
                BackgroundColor(colors::INK_SOFT),
                Pickable::IGNORE
            )],
        )],
    )
}
