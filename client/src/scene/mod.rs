//! Dark animus-style scene - minimal grid floor fading into void
use crate::*;
use avian3d::prelude::*;
use bevy_skein::SkeinPlugin;

pub fn plugin(app: &mut App) {
    app.add_plugins((PhysicsPlugins::default(), SkeinPlugin::default()))
    .add_systems(OnEnter(Screen::Gameplay), setup_animus_scene);
}

/// Dark animus scene — near-black floor with faintly glowing grid lines
fn setup_animus_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Large floor plane
    let floor_size = 500.0;
    let floor_mesh = meshes.add(Plane3d::default().mesh().size(floor_size, floor_size));

    let floor_material = materials.add(StandardMaterial {
        base_color: colors::NEUTRAL920,
        perceptual_roughness: 0.9,
        metallic: 0.0,
        reflectance: 0.05,
        ..default()
    });

    // Spawn floor with collision
    commands.spawn((
        Name::new("AnimusFloor"),
        DespawnOnExit(Screen::Gameplay),
        Mesh3d(floor_mesh),
        MeshMaterial3d(floor_material),
        Transform::from_translation(Vec3::ZERO),
        Collider::half_space(Vec3::Y),
        RigidBody::Static,
    ));

    // Grid lines - much larger extent for "infinite" feel
    let grid_color = ui::colors::NEUTRAL900;
    let grid_material = materials.add(StandardMaterial {
        base_color: grid_color,
        emissive: LinearRgba::from(grid_color),
        unlit: true,
        ..default()
    });

    let line_thickness = 0.025;
    let grid_spacing = 2.0;

    // Smaller grid for WASM to avoid OOM
    #[cfg(target_arch = "wasm32")]
    let grid_extent = 60.0;
    #[cfg(not(target_arch = "wasm32"))]
    let grid_extent = 200.0;

    let num_lines = (grid_extent / grid_spacing) as i32;

    let line_mesh = meshes.add(Cuboid::new(line_thickness, 0.001, grid_extent * 2.0));
    let line_mesh_z = meshes.add(Cuboid::new(grid_extent * 2.0, 0.001, line_thickness));

    for i in (-num_lines)..=num_lines {
        let offset = i as f32 * grid_spacing;

        // Lines along X axis
        commands.spawn((
            DespawnOnExit(Screen::Gameplay),
            Mesh3d(line_mesh.clone()),
            MeshMaterial3d(grid_material.clone()),
            Transform::from_translation(Vec3::new(offset, 0.005, 0.0)),
        ));

        // Lines along Z axis
        commands.spawn((
            DespawnOnExit(Screen::Gameplay),
            Mesh3d(line_mesh_z.clone()),
            MeshMaterial3d(grid_material.clone()),
            Transform::from_translation(Vec3::new(0.0, 0.005, offset)),
        ));
    }

    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 1500.0,
        ..Default::default()
    });

    commands.spawn((
        DespawnOnExit(Screen::Gameplay),
        DirectionalLight {
            color: Color::WHITE,
            illuminance: 4000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.7, 0.3, 0.0)),
    ));

    commands.insert_resource(ClearColor(colors::VOID));
}
