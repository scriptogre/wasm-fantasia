use super::*;
use avian3d::prelude::{Collider, LockedAxes, Mass, RigidBody};
use bevy_enhanced_input::prelude::Start;
use crate::models::SpawnEnemy;

pub fn plugin(app: &mut App) {
    app.add_observer(spawn_enemy_in_front);
}

/// Spawn an enemy in front of the player when E is pressed.
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
    let enemy_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.8, 0.2, 0.2),
        ..default()
    });

    // Spawn 3 units in front of the player
    let spawn_pos = player_transform.translation + player_transform.forward() * 3.0;

    commands.spawn((
        Name::new("TestEnemy"),
        DespawnOnExit(Screen::Gameplay),
        Transform::from_translation(spawn_pos),
        Mesh3d(enemy_mesh),
        MeshMaterial3d(enemy_material),
        // Combat components
        Health::new(1000.0),
        Enemy,
        Combatant,
        // Physics - heavy so player can't push easily
        Collider::capsule(0.5, 1.0),
        RigidBody::Dynamic,
        LockedAxes::ROTATION_LOCKED,
        Mass(500.0),
    ));

    info!("Spawned enemy at {:?}", spawn_pos);
}
