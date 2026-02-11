use super::*;
use bevy::window::{PresentMode, PrimaryWindow};
use bevy_seedling::prelude::*;

pub(super) fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            update_general_volume_label,
            update_music_volume_label,
            update_sfx_volume_label,
            update_fov_label,
            update_tab_content.run_if(resource_changed::<ActiveTab>),
        ),
    );
}

markers!(
    GeneralVolumeLabel,
    MusicVolumeLabel,
    SfxVolumeLabel,
    SaveSettingsLabel,
    VsyncLabel,
    FovLabel,
    TabBar,
    TabContent,
    ScreenShakeLabel
);
#[cfg(feature = "dev")]
markers!(DiagnosticsLabel, DebugUiLabel);

// ============================ CONTROL KNOBS OBSERVERS ============================

pub fn save_settings(
    _: On<Pointer<Click>>,
    settings: Res<Settings>,
    children_q: Query<&Children>,
    root: Query<&Children, With<SaveSettingsLabel>>,
    mut text_q: Query<&mut Text>,
) {
    // TODO: this is an insane nesting, improve it
    match settings.save() {
        Ok(()) => {
            info!("writing settings to '{SETTINGS_PATH}'");
            if let Ok(children) = root.single() {
                for child in children.iter() {
                    if let Ok(grandchildren) = children_q.get(child) {
                        for gc in grandchildren.iter() {
                            if let Ok(mut label) = text_q.get_mut(gc) {
                                label.0 = "Saved!".to_string();
                            }
                        }
                    }
                }
            }
        }
        Err(e) => error!("unable to write settings to '{SETTINGS_PATH}': {e}"),
    }
}

// TAB CHANGING
fn update_tab_content(
    session: Res<Session>,
    active_tab: Res<ActiveTab>,
    tab_bar: Query<&Children, With<TabBar>>,
    mut tab_content: Query<(Entity, &Children), With<TabContent>>,
    buttons: Query<(&UiTab, &Children)>,
    mut style_q: Query<(
        &mut PaletteSet,
        &mut BackgroundColor,
        &mut BorderColor,
        &Children,
    )>,
    mut text_color_q: Query<&mut TextColor>,
    mut commands: Commands,
) -> Result {
    for children in &tab_bar {
        for &child in children {
            let Ok((tab, btn_children)) = buttons.get(child) else {
                continue;
            };
            let is_active = *tab == active_tab.0;

            // Update PaletteSet + immediate colors on the "Button Content" child
            for &btn_child in btn_children {
                if let Ok((mut palette, mut bg, mut border, content_children)) =
                    style_q.get_mut(btn_child)
                {
                    let new_palette = if is_active {
                        PaletteSet {
                            none: Palette::new(
                                colors::NEUTRAL100,
                                colors::NEUTRAL800,
                                BorderColor::all(colors::NEUTRAL700),
                            ),
                            hovered: Palette::new(
                                colors::NEUTRAL100,
                                colors::NEUTRAL750,
                                BorderColor::all(colors::NEUTRAL650),
                            ),
                            pressed: Palette::new(
                                colors::NEUTRAL100,
                                colors::NEUTRAL700,
                                BorderColor::all(colors::NEUTRAL600),
                            ),
                            disabled: Palette::new(
                                colors::NEUTRAL400,
                                colors::NEUTRAL800,
                                BorderColor::all(colors::NEUTRAL700),
                            ),
                        }
                    } else {
                        PaletteSet::default()
                    };

                    bg.0 = new_palette.none.bg;
                    *border = new_palette.none.border;
                    let text_color = new_palette.none.text;
                    *palette = new_palette;

                    for &text_child in content_children {
                        if let Ok(mut tc) = text_color_q.get_mut(text_child) {
                            tc.0 = text_color;
                        }
                    }
                }
            }

            if is_active {
                let (e, content) = tab_content.single_mut()?;
                for child in content.iter() {
                    commands.entity(child).despawn();
                }
                match tab {
                    UiTab::Audio => {
                        commands.spawn(audio_grid()).insert(ChildOf(e));
                    }
                    UiTab::Video => {
                        commands.spawn(video_grid(&session)).insert(ChildOf(e));
                    }
                }
            }
        }
    }

    Ok(())
}

// ============================ +/- BUTTON HOOKS ============================

fn fov_lower(
    _: On<Pointer<Click>>,
    cfg: Res<Config>,
    mut settings: ResMut<Settings>,
    mut world_model_projection: Single<&mut Projection>,
) {
    let Projection::Perspective(perspective) = world_model_projection.as_mut() else {
        return;
    };
    let new_fov = (settings.fov - cfg.settings.step.to_degrees()).max(cfg.settings.min_fov);
    perspective.fov = new_fov.to_radians();
    settings.fov = perspective.fov.to_degrees();
}

fn fov_raise(
    _: On<Pointer<Click>>,
    cfg: Res<Config>,
    mut settings: ResMut<Settings>,
    mut world_model_projection: Single<&mut Projection>,
) {
    let Projection::Perspective(perspective) = world_model_projection.as_mut() else {
        return;
    };
    let new_fov = (settings.fov + cfg.settings.step.to_degrees()).min(cfg.settings.max_fov);
    perspective.fov = new_fov.to_radians();
    settings.fov = perspective.fov.to_degrees();
}

fn update_fov_label(settings: Res<Settings>, mut label: Single<&mut Text, With<FovLabel>>) {
    let fov = settings.fov.round();
    let text = format!("{fov: <3}"); // pad to 3 chars
    label.0 = text;
}

// GENERAL
fn general_lower(
    _: On<Pointer<Click>>,
    cfg: ResMut<Config>,
    mut settings: ResMut<Settings>,
    mut general: Single<&mut VolumeNode, With<MainBus>>,
) {
    let new_volume = (settings.sound.general - cfg.settings.step).max(cfg.settings.min_volume);
    settings.sound.general = new_volume;
    general.volume = Volume::Linear(new_volume);
}

fn general_raise(
    _: On<Pointer<Click>>,
    cfg: ResMut<Config>,
    mut settings: ResMut<Settings>,
    mut general: Single<&mut VolumeNode, With<MainBus>>,
) {
    let new_volume = (settings.sound.general + cfg.settings.step).min(cfg.settings.max_volume);
    settings.sound.general = new_volume;
    general.volume = Volume::Linear(new_volume);
}

fn update_general_volume_label(
    settings: Res<Settings>,
    mut label: Single<&mut Text, With<GeneralVolumeLabel>>,
) {
    let percent = (settings.sound.general * 100.0).round();
    let text = format!("{percent: <3}%"); // pad the percent to 3 chars
    label.0 = text;
}

// MUSIC
fn music_lower(
    _: On<Pointer<Click>>,
    cfg: ResMut<Config>,
    mut settings: ResMut<Settings>,
    mut music: Single<&mut VolumeNode, With<SamplerPool<MusicPool>>>,
) {
    let new_volume = (settings.sound.music - cfg.settings.step).max(cfg.settings.min_volume);
    settings.sound.music = new_volume;
    music.volume = settings.music();
}

fn music_raise(
    _: On<Pointer<Click>>,
    cfg: ResMut<Config>,
    mut settings: ResMut<Settings>,
    mut music: Single<&mut VolumeNode, With<SamplerPool<MusicPool>>>,
) {
    let new_volume = (settings.sound.music + cfg.settings.step).min(cfg.settings.max_volume);
    settings.sound.music = new_volume;
    music.volume = settings.music();
}

fn update_music_volume_label(
    settings: Res<Settings>,
    mut label: Single<&mut Text, With<MusicVolumeLabel>>,
) {
    let percent = (settings.sound.music * 100.0).round();
    let text = format!("{percent: <3}%"); // pad the percent to 3 chars
    label.0 = text;
}

// SFX
fn sfx_lower(
    _: On<Pointer<Click>>,
    cfg: ResMut<Config>,
    mut settings: ResMut<Settings>,
    mut sfx: Single<&mut VolumeNode, With<SoundEffectsBus>>,
) {
    let new_volume = (settings.sound.sfx - cfg.settings.step).max(cfg.settings.min_volume);
    settings.sound.sfx = new_volume;
    sfx.volume = settings.sfx();
}

fn sfx_raise(
    _: On<Pointer<Click>>,
    cfg: ResMut<Config>,
    mut settings: ResMut<Settings>,
    mut sfx: Single<&mut VolumeNode, With<SoundEffectsBus>>,
) {
    let new_volume = (settings.sound.sfx + cfg.settings.step).min(cfg.settings.max_volume);
    settings.sound.sfx = new_volume;
    sfx.volume = settings.sfx();
}

fn update_sfx_volume_label(
    settings: Res<Settings>,
    mut label: Single<&mut Text, With<SfxVolumeLabel>>,
) {
    let percent = (settings.sound.sfx * 100.0).round();
    let text = format!("{percent: <3}%"); // pad the percent to 3 chars
    label.0 = text;
}

// ============================ OTHER BUTTON HOOKS ============================

fn switch_to_tab(tab: UiTab) -> impl Fn(On<Pointer<Click>>, ResMut<ActiveTab>) + Clone {
    move |_: On<Pointer<Click>>, mut active_tab: ResMut<ActiveTab>| {
        active_tab.0 = tab;
    }
}

fn click_toggle_vsync(
    _: On<Pointer<Click>>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) -> Result {
    for mut window in windows.iter_mut() {
        if matches!(window.present_mode, PresentMode::AutoVsync) {
            window.present_mode = PresentMode::AutoNoVsync;
        } else {
            window.present_mode = PresentMode::AutoVsync;
        }
        info!(" window present_mode changed to: {:?}", window.present_mode);
    }

    Ok(())
}

/// Helper to find and update Text in button descendants
fn update_button_text(
    root: Entity,
    new_text: &str,
    children_q: &Query<&Children>,
    text_q: &mut Query<&mut Text>,
) {
    // Try direct first (in case root has Text)
    if let Ok(mut text) = text_q.get_mut(root) {
        text.0 = new_text.to_owned();
        return;
    }
    // Traverse children
    if let Ok(children) = children_q.get(root) {
        for child in children.iter() {
            update_button_text(child, new_text, children_q, text_q);
        }
    }
}

#[cfg(feature = "dev")]
fn click_toggle_diagnostics(
    _: On<Pointer<Click>>,
    mut state: ResMut<Session>,
    buttons: Query<Entity, With<DiagnosticsLabel>>,
    children_q: Query<&Children>,
    mut text_q: Query<&mut Text>,
) {
    state.diagnostics = !state.diagnostics;
    let label = if state.diagnostics { "on" } else { "off" };

    for button in buttons.iter() {
        update_button_text(button, label, &children_q, &mut text_q);
    }
}

#[cfg(feature = "dev")]
fn click_toggle_debug_ui(
    _: On<Pointer<Click>>,
    mut commands: Commands,
    mut state: ResMut<Session>,
    buttons: Query<Entity, With<DebugUiLabel>>,
    children_q: Query<&Children>,
    mut text_q: Query<&mut Text>,
) {
    state.debug_ui = !state.debug_ui;
    commands.trigger(ToggleDebugUi);
    let label = if state.debug_ui { "on" } else { "off" };

    for button in buttons.iter() {
        update_button_text(button, label, &children_q, &mut text_q);
    }
}

fn click_toggle_screen_shake(
    _: On<Pointer<Click>>,
    mut state: ResMut<Session>,
    buttons: Query<Entity, With<ScreenShakeLabel>>,
    children_q: Query<&Children>,
    mut text_q: Query<&mut Text>,
) {
    state.screen_shake = !state.screen_shake;
    let label = if state.screen_shake { "on" } else { "off" };

    for button in buttons.iter() {
        update_button_text(button, label, &children_q, &mut text_q);
    }
}

fn click_toggle_settings(
    click: On<Pointer<Click>>,
    mut commands: Commands,
    screen: Res<State<Screen>>,
    mut next_screen: ResMut<NextState<Screen>>,
) {
    if *screen.get() == Screen::Settings {
        next_screen.set(Screen::Title);
    } else {
        commands.entity(click.event_target()).trigger(PopModal);
    }
}

// ============================ UI ============================

pub fn settings_ui() -> impl Bundle {
    (
        ui_root("Settings Screen"),
        GlobalZIndex(200),
        children![(
            Node {
                width: Percent(80.0),
                height: Percent(80.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            children![
                tab_bar(),
                (TabContent, Node::default(), children![audio_grid()]),
                bottom_row()
            ]
        )],
    )
}

fn tab_bar() -> impl Bundle {
    let r = size::BORDER_RADIUS;
    let z = Px(0.0);
    let left_tab = Props::default()
        .text("Audio")
        .border_radius_custom(BorderRadius::new(r, z, z, r));
    let right_tab = Props::default()
        .text("Video")
        .border_radius_custom(BorderRadius::new(z, r, r, z));
    (
        Node {
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            position_type: PositionType::Absolute,
            width: Percent(100.0),
            top: Vh(2.0),
            row_gap: Vh(2.0),
            ..default()
        },
        children![
            header("Settings"),
            (
                Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::Center,
                    width: Percent(100.0),
                    ..default()
                },
                TabBar,
                children![
                    (btn(left_tab, switch_to_tab(UiTab::Audio)), UiTab::Audio),
                    (btn(right_tab, switch_to_tab(UiTab::Video)), UiTab::Video),
                ],
            ),
        ],
    )
}

fn bottom_row() -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::Center,
            column_gap: Px(16.0),
            bottom: Vh(1.0),
            ..default()
        },
        children![
            (btn("Save", save_settings), SaveSettingsLabel),
            btn("Back", click_toggle_settings),
        ],
    )
}

fn video_grid(state: &Session) -> impl Bundle {
    let screen_shake_label = if state.screen_shake { "on" } else { "off" };

    #[cfg(feature = "dev")]
    let diagnostics_label = if state.diagnostics { "on" } else { "off" };
    #[cfg(feature = "dev")]
    let debug_ui_label = if state.debug_ui { "on" } else { "off" };

    (
        Name::new("Settings Video Grid"),
        Node {
            row_gap: Px(14.0),
            column_gap: Px(30.0),
            display: Display::Grid,
            grid_template_columns: RepeatedGridTrack::px(2, 240.0),
            align_items: AlignItems::Center,
            justify_items: JustifyItems::Center,
            ..default()
        },
        #[cfg(not(feature = "dev"))]
        children![
            label("FOV"),
            plus_minus_bar(FovLabel, fov_lower, fov_raise),
            label("VSync"),
            (btn("on", click_toggle_vsync), VsyncLabel),
            label("Screen Shake"),
            (
                btn(screen_shake_label, click_toggle_screen_shake),
                ScreenShakeLabel
            ),
        ],
        #[cfg(feature = "dev")]
        children![
            label("FOV"),
            plus_minus_bar(FovLabel, fov_lower, fov_raise),
            label("VSync"),
            (btn("on", click_toggle_vsync), VsyncLabel),
            label("Screen Shake"),
            (
                btn(screen_shake_label, click_toggle_screen_shake),
                ScreenShakeLabel
            ),
            label("Diagnostics"),
            (
                btn(diagnostics_label, click_toggle_diagnostics),
                DiagnosticsLabel
            ),
            label("Debug UI"),
            (btn(debug_ui_label, click_toggle_debug_ui), DebugUiLabel),
        ],
    )
}

fn audio_grid() -> impl Bundle {
    (
        Name::new("Settings Audio Grid"),
        Node {
            row_gap: Px(14.0),
            column_gap: Px(30.0),
            display: Display::Grid,
            grid_template_columns: RepeatedGridTrack::px(2, 240.0),
            align_items: AlignItems::Center,
            justify_items: JustifyItems::Center,
            ..default()
        },
        children![
            label("General"),
            plus_minus_bar(GeneralVolumeLabel, general_lower, general_raise),
            label("Music"),
            plus_minus_bar(MusicVolumeLabel, music_lower, music_raise),
            label("SFX"),
            plus_minus_bar(SfxVolumeLabel, sfx_lower, sfx_raise),
        ],
    )
}
