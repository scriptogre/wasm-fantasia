use super::*;
use crate::models::SpawnEnemy;
use crate::rules::{Stat, Stats};
use avian3d::prelude::{Collider, LockedAxes, Mass, RigidBody};
use bevy_enhanced_input::prelude::Start;

pub fn plugin(app: &mut App) {
    app.add_observer(spawn_enemy_in_front);
}

/// Spawn a pack of enemies in front of the player when E is pressed.
/// In multiplayer: calls server reducer so all clients see the enemies.
/// Offline: spawns locally like before.
fn spawn_enemy_in_front(
    _on: On<Start<SpawnEnemy>>,
    player: Query<&Transform, With<Player>>,
    #[cfg(not(target_arch = "wasm32"))] conn: Option<Res<crate::networking::SpacetimeDbConnection>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Ok(player_transform) = player.single() else {
        return;
    };

    let forward = player_transform.forward();
    let pos = player_transform.translation;

    // If multiplayer is connected, spawn via server
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(conn) = conn {
        crate::networking::combat::server_spawn_enemies(
            &conn,
            pos,
            forward.as_vec3(),
        );
        info!("Requested 5 enemies from server");
        return;
    }

    // Offline fallback: spawn locally
    let enemy_mesh = meshes.add(Capsule3d::new(0.5, 1.0));
    let right = player_transform.right();
    let base_pos = pos + *forward * 5.0;

    let offsets = [
        Vec3::ZERO,
        *right * 1.5 + *forward * -0.5,
        *right * -1.5 + *forward * -0.5,
        *right * 2.5 + *forward * -1.5,
        *right * -2.5 + *forward * -1.5,
    ];

    for (i, offset) in offsets.iter().enumerate() {
        let spawn_pos = base_pos + *offset;

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
            Health::new(500.0),
            Enemy,
            Combatant,
            Stats::new()
                .with(Stat::MaxHealth, 500.0)
                .with(Stat::Health, 500.0),
            Collider::capsule(0.5, 1.0),
            RigidBody::Dynamic,
            LockedAxes::ROTATION_LOCKED,
            Mass(500.0),
        ));
    }

    info!("Spawned 5 enemies locally (offline mode)");
}
