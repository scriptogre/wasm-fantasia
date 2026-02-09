use super::*;
use crate::ui::Modals;
use bevy_third_person_camera::*;

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::Gameplay), add_tpv_cam)
        .add_systems(OnExit(Screen::Gameplay), rm_tpv_cam.before(GameplayCleanup))
        .add_observer(toggle_cam_cursor);
}

fn add_tpv_cam(
    cfg: Res<Config>,
    mut commands: Commands,
    mut camera: Query<(Entity, &mut Transform), With<SceneCamera>>,
    mut tpv_cam: Query<Entity, With<ThirdPersonCamera>>,
) -> Result {
    let Ok((cam, mut transform)) = camera.single_mut() else {
        return Ok(());
    };
    if tpv_cam.single_mut().is_ok() {
        debug!("Tried to add ThirdPersonCamera to an entity that already has it");
        return Ok(());
    }

    // Set initial camera rotation to ~45 degrees pitch (looking down at player)
    // Balanced angle - good visibility without feeling too top-down
    let pitch = 45_f32.to_radians();
    transform.rotation = Quat::from_rotation_x(-pitch);

    commands.entity(cam).insert((
        ThirdPersonCamera {
            zoom_enabled: true,
            zoom: Zoom::new(cfg.player.zoom.0, cfg.player.zoom.1),
            zoom_sensitivity: 0.2, // Reduced from default ~1.0 for trackpad
            offset_enabled: false, // disable shoulder offset for top-down-ish view
            cursor_lock_key: KeyCode::KeyL,
            gamepad_settings: CustomGamepadSettings::default(),
            ..default()
        },
        Projection::from(PerspectiveProjection {
            fov: cfg.player.fov.to_radians(),
            ..Default::default()
        }),
    ));

    Ok(())
}

fn rm_tpv_cam(
    mut commands: Commands,
    mut camera: Query<(Entity, &mut Transform), With<ThirdPersonCamera>>,
) {
    if let Ok((entity, mut transform)) = camera.single_mut() {
        commands.entity(entity).remove::<ThirdPersonCamera>();
        // Point camera at nothing so gameplay scene doesn't bleed into title screen
        *transform = Transform::from_xyz(0.0, 50.0, 0.0).looking_at(Vec3::ZERO, Vec3::Y);
    }
}

fn toggle_cam_cursor(
    _: On<CamCursorToggle>,
    mut cam: Query<&mut ThirdPersonCamera>,
    modals: Res<Modals>,
) {
    let Ok(mut cam) = cam.single_mut() else {
        return;
    };
    // Explicitly set based on modal state — avoids desync from double-fires
    cam.cursor_lock_active = modals.is_empty();
}
