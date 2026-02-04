//! Animus-style loading scene - minimal grid floor fading into void
use crate::*;
use avian3d::prelude::*;
use bevy_skein::SkeinPlugin;

mod skybox;
pub use skybox::*;

pub fn plugin(app: &mut App) {
    app.add_plugins((
        PhysicsPlugins::default(),
        SkeinPlugin::default(),
        // skybox::plugin, // Disabled for clean Animus look
    ))
    .add_systems(OnEnter(Screen::Title), setup_animus_scene);
}

/// Creates an Assassin's Creed Animus-style loading scene
pub fn setup_animus_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Large floor plane
    let floor_size = 200.0;
    let floor_mesh = meshes.add(Plane3d::default().mesh().size(floor_size, floor_size));

    // Clean white/light gray floor material
    let floor_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.85, 0.85, 0.88),
        perceptual_roughness: 0.9,
        metallic: 0.0,
        reflectance: 0.1,
        ..default()
    });

    // Spawn floor with collision
    commands.spawn((
        Name::new("AnimusFloor"),
        Mesh3d(floor_mesh),
        MeshMaterial3d(floor_material),
        Transform::from_translation(Vec3::ZERO),
        Collider::half_space(Vec3::Y),
        RigidBody::Static,
    ));

    // Grid lines on the floor
    let grid_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.6, 0.6, 0.65, 0.5),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    // Create grid lines
    let line_thickness = 0.02;
    let grid_spacing = 2.0;
    let grid_extent = 50.0;
    let line_mesh = meshes.add(Cuboid::new(line_thickness, 0.001, grid_extent * 2.0));
    let line_mesh_z = meshes.add(Cuboid::new(grid_extent * 2.0, 0.001, line_thickness));

    for i in (-25i32)..=25 {
        let offset = i as f32 * grid_spacing;

        // Lines along X axis
        commands.spawn((
            Mesh3d(line_mesh.clone()),
            MeshMaterial3d(grid_material.clone()),
            Transform::from_translation(Vec3::new(offset, 0.001, 0.0)),
        ));

        // Lines along Z axis
        commands.spawn((
            Mesh3d(line_mesh_z.clone()),
            MeshMaterial3d(grid_material.clone()),
            Transform::from_translation(Vec3::new(0.0, 0.001, offset)),
        ));
    }

    // Bright ambient light for that clean Animus look
    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 2000.0,
        ..Default::default()
    });

    // Soft directional light
    commands.spawn((
        DirectionalLight {
            color: Color::WHITE,
            illuminance: 5000.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.5, 0.5, 0.0)),
    ));

    // Clean white/gray background (fog effect)
    commands.insert_resource(ClearColor(Color::srgb(0.92, 0.92, 0.95)));
}
