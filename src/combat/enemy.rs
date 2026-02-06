use super::*;
use crate::models::SpawnEnemy;
use crate::rules::{Stat, Stats};
use avian3d::prelude::{Collider, LockedAxes, Mass, RigidBody};
use bevy_enhanced_input::prelude::Start;

pub fn plugin(app: &mut App) {
    app.add_observer(spawn_enemy_in_front);
}

/// Spawn a pack of enemies in front of the player when E is pressed.
fn spawn_enemy_in_front(
    _on: On<Start<SpawnEnemy>>,
    player: Query<&Transform, With<Player>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Ok(player_transform) = player.single() else {
        return;
    };

    let enemy_mesh = meshes.add(Capsule3d::new(0.5, 1.0));

    // Spawn 5 enemies in a spread formation in front of the player
    let forward = player_transform.forward();
    let right = player_transform.right();
    let base_pos = player_transform.translation + *forward * 5.0;

    // Formation: spread in an arc
    let offsets = [
        Vec3::ZERO,                      // Center
        *right * 1.5 + *forward * -0.5,  // Right
        *right * -1.5 + *forward * -0.5, // Left
        *right * 2.5 + *forward * -1.5,  // Far right
        *right * -2.5 + *forward * -1.5, // Far left
    ];

    for (i, offset) in offsets.iter().enumerate() {
        let spawn_pos = base_pos + *offset;

        // Each enemy needs its own material for hit flash to work correctly
        let enemy_material = materials.add(StandardMaterial {
            base_color: Color::srgb(0.8, 0.2, 0.2),
            ..default()
        });

        commands.spawn((
            Name::new(format!("TestEnemy_{}", i)),
            DespawnOnExit(Screen::Gameplay),
            Transform::from_translation(spawn_pos),
            Mesh3d(enemy_mesh.clone()),
            MeshMaterial3d(enemy_material),
            // Combat components
            Health::new(500.0),
            Enemy,
            Combatant,
            // Stats for rules system
            Stats::new()
                .with(Stat::MaxHealth, 500.0)
                .with(Stat::Health, 500.0),
            // Physics
            Collider::capsule(0.5, 1.0),
            RigidBody::Dynamic,
            LockedAxes::ROTATION_LOCKED,
            Mass(500.0),
        ));
    }

    info!("Spawned 5 enemies in front of player");
}
