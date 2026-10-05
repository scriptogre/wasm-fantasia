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
            update_tab_content
                .run_if(resource_changed::<ActiveTab>.or(any_match_filter::<Added<TabBar>>)),
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
    ScreenShakeLabel,
    DiagnosticsLabel
);
#[cfg(feature = "dev")]
markers!(DebugUiLabel);

// ============================ CONTROL KNOBS OBSERVERS ============================

pub fn save_settings(
    _: On<Pointer<Click>>,
    settings: Res<Settings>,
    children_q: Query<&Children>,
    root: Query<&Children, With<SaveSettingsLabel>>,
    mut text_q: Query<&mut Text>,
) {
    let status = match settings.save() {
        Ok(()) => {
            info!("Saved settings to {SETTINGS_LOCATION}");
            "Saved"
        }
        Err(e) => {
            error!("Unable to save settings to {SETTINGS_LOCATION}: {e}");
            "Save failed"
        }
    };

    let Ok(children) = root.single() else {
        return;
    };
    for gc in children
        .iter()
        .filter_map(|child| children_q.get(child).ok())
        .flat_map(|grandchildren| grandchildren.iter())
    {
        if let Ok(mut label) = text_q.get_mut(gc) {
            label.0 = status.to_string();
        }
    }
}

// TAB CHANGING
fn update_tab_content(
    session: Res<Session>,
    active_tab: Res<ActiveTab>,
    tab_bar: Query<&Children, With<TabBar>>,
    tab_content: Query<Entity, With<TabContent>>,
    buttons: Query<(&UiTab, &Children)>,
    mut style_q: Query<(
        &mut PaletteSet,
        &mut BackgroundColor,
        &mut BorderColor,
        &Children,
    )>,
    mut text_color_q: Query<&mut TextColor>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut commands: Commands,
) -> Result {
    for children in &tab_bar {
        for &child in children {
            let Ok((tab, btn_children)) = buttons.get(child) else {
                continue;
            };
            let is_active = *tab == active_tab.0;

            // Selected tab uses the filled accent, the other stays ghost
            for &btn_child in btn_children {
                if let Ok((mut palette, mut bg, mut border, content_children)) =
                    style_q.get_mut(btn_child)
                {
                    *palette = if is_active {
                        PaletteSet::primary()
                    } else {
                        PaletteSet::default()
                    };
                    bg.0 = palette.none.bg;
                    *border = palette.none.border;
                    for &text_child in content_children {
                        if let Ok(mut tc) = text_color_q.get_mut(text_child) {
                            tc.0 = palette.none.text;
                        }
                    }
                }
            }

            if is_active {
                let e = tab_content.single()?;
                commands.entity(e).despawn_children();
                match tab {
                    UiTab::Audio => {
                        commands.spawn(audio_grid()).insert(ChildOf(e));
                    }
                    UiTab::Video => {
                        let vsync_on = windows
                            .single()
                            .map(|w| matches!(w.present_mode, PresentMode::AutoVsync))
                            .unwrap_or(true);
                        commands
                            .spawn(video_grid(&session, vsync_on))
                            .insert(ChildOf(e));
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
    buttons: Query<Entity, With<VsyncLabel>>,
    children_q: Query<&Children>,
    mut text_q: Query<&mut Text>,
) -> Result {
    for mut window in windows.iter_mut() {
        if matches!(window.present_mode, PresentMode::AutoVsync) {
            window.present_mode = PresentMode::AutoNoVsync;
        } else {
            window.present_mode = PresentMode::AutoVsync;
        }
        info!(" window present_mode changed to: {:?}", window.present_mode);

        let label = if matches!(window.present_mode, PresentMode::AutoVsync) {
            "On"
        } else {
            "Off"
        };
        for entity in &buttons {
            update_button_text(entity, label, &children_q, &mut text_q);
        }
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

fn click_toggle_diagnostics(
    _: On<Pointer<Click>>,
    mut state: ResMut<Session>,
    buttons: Query<Entity, With<DiagnosticsLabel>>,
    children_q: Query<&Children>,
    mut text_q: Query<&mut Text>,
) {
    state.diagnostics = !state.diagnostics;
    let label = if state.diagnostics { "On" } else { "Off" };

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
    let label = if state.debug_ui { "On" } else { "Off" };

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
    let label = if state.screen_shake { "On" } else { "Off" };

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

/// Right column width: matches the -/+ spinner (44 + 4 + 48 + 4 + 44), so toggles line up with it
const CONTROL_WIDTH: Val = Px(144.0);

pub fn settings_ui() -> impl Bundle {
    (
        ui_root("Settings Screen"),
        GlobalZIndex(200),
        children![(
            panel(480.0),
            // No header: the tabs say where you are, and landscape phones need the height
            children![
                tab_bar(),
                (
                    TabContent,
                    Node {
                        // Four rows, so switching tabs doesn't resize the panel
                        min_height: Px(188.0),
                        ..default()
                    },
                ),
                bottom_row()
            ]
        )],
    )
}

fn tab_bar() -> impl Bundle {
    let r = size::BORDER_RADIUS;
    let z = Px(0.0);
    let left_tab = Props::new("Audio").border_radius_custom(BorderRadius::new(r, z, z, r));
    let right_tab = Props::new("Video").border_radius_custom(BorderRadius::new(z, r, r, z));
    (
        TabBar,
        two_columns(),
        children![
            (btn(left_tab, switch_to_tab(UiTab::Audio)), UiTab::Audio),
            (btn(right_tab, switch_to_tab(UiTab::Video)), UiTab::Video),
        ],
    )
}

fn bottom_row() -> impl Bundle {
    (
        Node {
            column_gap: Px(12.0),
            ..two_columns()
        },
        children![
            (btn("Save", save_settings), SaveSettingsLabel),
            btn(
                Props::new("Back").palette_set(PaletteSet::primary()),
                click_toggle_settings
            ),
        ],
    )
}

fn two_columns() -> Node {
    Node {
        display: Display::Grid,
        grid_template_columns: RepeatedGridTrack::flex(2, 1.0),
        ..default()
    }
}

/// Label on the left, control on the right
fn settings_grid() -> Node {
    Node {
        width: Percent(100.0),
        display: Display::Grid,
        grid_template_columns: vec![GridTrack::flex(1.0), GridTrack::auto()],
        row_gap: Px(4.0),
        column_gap: Px(12.0),
        align_items: AlignItems::Center,
        justify_items: JustifyItems::Start,
        ..default()
    }
}

fn row(text: &'static str) -> impl Bundle {
    label(Props::new(text).color(colors::NEUTRAL300))
}

fn toggle(text: &'static str) -> Props {
    Props::new(text).width(CONTROL_WIDTH)
}

fn video_grid(state: &Session, vsync_on: bool) -> impl Bundle {
    let vsync_label = if vsync_on { "On" } else { "Off" };
    let screen_shake_label = if state.screen_shake { "On" } else { "Off" };
    let diagnostics_label = if state.diagnostics { "On" } else { "Off" };
    #[cfg(feature = "dev")]
    let debug_ui_label = if state.debug_ui { "On" } else { "Off" };

    (
        Name::new("Settings Video Grid"),
        settings_grid(),
        #[cfg(not(feature = "dev"))]
        children![
            row("FOV"),
            plus_minus_bar(FovLabel, fov_lower, fov_raise),
            row("VSync"),
            (btn(toggle(vsync_label), click_toggle_vsync), VsyncLabel),
            row("Screen Shake"),
            (
                btn(toggle(screen_shake_label), click_toggle_screen_shake),
                ScreenShakeLabel
            ),
            row("Diagnostics"),
            (
                btn(toggle(diagnostics_label), click_toggle_diagnostics),
                DiagnosticsLabel
            ),
        ],
        #[cfg(feature = "dev")]
        children![
            row("FOV"),
            plus_minus_bar(FovLabel, fov_lower, fov_raise),
            row("VSync"),
            (btn(toggle(vsync_label), click_toggle_vsync), VsyncLabel),
            row("Screen Shake"),
            (
                btn(toggle(screen_shake_label), click_toggle_screen_shake),
                ScreenShakeLabel
            ),
            row("Diagnostics"),
            (
                btn(toggle(diagnostics_label), click_toggle_diagnostics),
                DiagnosticsLabel
            ),
            row("Debug UI"),
            (
                btn(toggle(debug_ui_label), click_toggle_debug_ui),
                DebugUiLabel
            ),
        ],
    )
}

fn audio_grid() -> impl Bundle {
    (
        Name::new("Settings Audio Grid"),
        settings_grid(),
        children![
            row("Master"),
            plus_minus_bar(GeneralVolumeLabel, general_lower, general_raise),
            row("Music"),
            plus_minus_bar(MusicVolumeLabel, music_lower, music_raise),
            row("Effects"),
            plus_minus_bar(SfxVolumeLabel, sfx_lower, sfx_raise),
        ],
    )
}
