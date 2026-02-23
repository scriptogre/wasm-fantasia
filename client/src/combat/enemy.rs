use super::*;
use crate::asset_loading::Models;
use crate::models::{ClearEnemies, SpawnEnemy};
use crate::rendering::{
    VatEnemyState, VatMeshLink, initialize_vat_enemy_resources, spawn_vat_mesh_child,
};
use bevy_enhanced_input::prelude::Start;
use bevy_open_vat::prelude::*;

/// Squared XZ distance beyond which enemies are culled (fog_end + 5)².
/// Enemies past the fog end are fully obscured, so hiding them is free.
#[cfg(target_arch = "wasm32")]
const CULL_DISTANCE_SQ: f32 = 3600.0; // (55 + 5)²
#[cfg(not(target_arch = "wasm32"))]
const CULL_DISTANCE_SQ: f32 = 24025.0; // (150 + 5)²

pub fn plugin(app: &mut App) {
    app.add_observer(spawn_enemy_in_front)
        .add_observer(clear_all_enemies)
        .add_observer(on_enemy_added)
        .add_systems(
            Update,
            (
                initialize_vat_enemy_resources
                    .run_if(not(resource_exists::<VatEnemyState>).and(in_state(Screen::Gameplay))),
                attach_vat_to_pending_enemies
                    .run_if(resource_exists::<VatEnemyState>.and(in_state(Screen::Gameplay))),
                animate_enemies
                    .in_set(PostPhysicsAppSystems::PlayAnimations)
                    .run_if(in_state(Screen::Gameplay)),
            ),
        )
        .add_systems(
            PostUpdate,
            cull_enemies_beyond_fog.run_if(in_state(Screen::Gameplay)),
        );
}

/// Marker for enemies that spawned before VatEnemyState was ready.
/// The `attach_vat_to_pending_enemies` system picks these up.
#[derive(Component)]
#[component(storage = "SparseSet")]
struct PendingVatSetup;

// =============================================================================
// Spawn trigger (E key / server request)
// =============================================================================

/// Spawn a pack of enemies via server reducer.
/// All game modes go through SpacetimeDB when connected.
fn spawn_enemy_in_front(
    _on: On<Start<SpawnEnemy>>,
    player: Query<&Transform, With<Player>>,
    conn: Option<Res<crate::networking::SpacetimeDbConnection>>,
) {
    let Ok(player_transform) = player.single() else {
        return;
    };

    let forward = player_transform.forward();
    let pos = player_transform.translation;

    if let Some(conn) = conn {
        use spacetimedb_sdk::DbContext;
        if conn.conn.is_active() {
            crate::networking::combat::server_spawn_enemies(&conn, pos, forward.as_vec3());
            debug!("Requested enemies from server");
            return;
        }
    }

    warn!("No server connection — cannot spawn enemies");
}

/// Delete all enemies in the current world via server reducer.
fn clear_all_enemies(
    _on: On<Start<ClearEnemies>>,
    conn: Option<Res<crate::networking::SpacetimeDbConnection>>,
) {
    if let Some(conn) = conn {
        use spacetimedb_sdk::DbContext;
        if conn.conn.is_active() {
            crate::networking::combat::server_clear_enemies(&conn);
            info!("Requested enemy clear from server");
            return;
        }
    }

    warn!("No server connection — cannot clear enemies");
}

// =============================================================================
// On<Add, Enemy> — attach VAT mesh directly (flat hierarchy)
// =============================================================================

fn on_enemy_added(
    on: On<Add, Enemy>,
    vat_state: Option<Res<VatEnemyState>>,
    models: Res<Models>,
    mut commands: Commands,
) {
    let entity = on.entity;

    // Remove capsule mesh if present (reconciler may have added it)
    commands
        .entity(entity)
        .remove::<Mesh3d>()
        .remove::<MeshMaterial3d<StandardMaterial>>();

    // No physics components — attack system uses manual distance checks,
    // and server handles all movement/knockback. Avian3d was costing ~8ms
    // for 5000 kinematic sensors.
    commands.entity(entity).insert((
        EnemyBehavior::default(),
        InheritedVisibility::default(),
    ));

    if let Some(vat_state) = vat_state {
        // VatEnemyState is ready — spawn mesh child directly (flat hierarchy)
        spawn_vat_mesh_child(&mut commands, entity, &vat_state, &models);
    } else {
        // Assets not loaded yet — mark for later setup
        commands.entity(entity).insert(PendingVatSetup);
    }
}

/// Catch-up system: attaches VAT mesh to enemies that were added before
/// VatEnemyState was ready.
fn attach_vat_to_pending_enemies(
    pending: Query<Entity, With<PendingVatSetup>>,
    vat_state: Res<VatEnemyState>,
    models: Res<Models>,
    mut commands: Commands,
) {
    for entity in &pending {
        commands.entity(entity).remove::<PendingVatSetup>();
        spawn_vat_mesh_child(&mut commands, entity, &vat_state, &models);
    }
}

// =============================================================================
// Animation driver — maps EnemyBehavior to VAT clip names
// =============================================================================

fn animate_enemies(
    enemies: Query<(&EnemyBehavior, &VatMeshLink), Changed<EnemyBehavior>>,
    mut controllers: Query<&mut VatAnimationController>,
    time: Res<Time>,
) {
    for (behavior, vat_link) in &enemies {
        let clip_name = match behavior {
            EnemyBehavior::Idle => "Zombie_Idle_Loop",
            EnemyBehavior::Chase => "Zombie_Walk_Fwd_Loop",
            EnemyBehavior::Attack => "Zombie_Scratch",
        };

        let now = time.elapsed_secs();
        for &mesh_entity in &vat_link.0 {
            if let Ok(mut controller) = controllers.get_mut(mesh_entity) {
                if controller.current_clip != clip_name {
                    controller.current_clip = clip_name.to_string();
                    controller.start_time = now;
                }
            }
        }
    }
}

// =============================================================================
// Distance culling — hide enemies fully obscured by fog
// =============================================================================

fn cull_enemies_beyond_fog(
    camera: Query<&Transform, With<SceneCamera>>,
    mut enemies: Query<(&Transform, &mut Visibility), With<Enemy>>,
) {
    let Ok(cam) = camera.single() else {
        return;
    };
    let cam_xz = Vec2::new(cam.translation.x, cam.translation.z);

    for (transform, mut visibility) in &mut enemies {
        let enemy_xz = Vec2::new(transform.translation.x, transform.translation.z);
        let dist_sq = cam_xz.distance_squared(enemy_xz);

        let desired = if dist_sq > CULL_DISTANCE_SQ {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };

        if *visibility != desired {
            *visibility = desired;
        }
    }
}
