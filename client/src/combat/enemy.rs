use super::*;
use avian3d::prelude::{Collider, RigidBody};
use bevy::mesh::MeshTag;
use bevy::pbr::ExtendedMaterial;
use bevy::render::storage::ShaderStorageBuffer;
use bevy::scene::SceneInstanceReady;
use bevy_enhanced_input::prelude::Start;
use bevy_open_vat::data::VatInstanceData;
use bevy_open_vat::prelude::*;

use crate::asset_loading::Models;
use crate::models::SpawnEnemy;

pub fn plugin(app: &mut App) {
    app.add_observer(spawn_enemy_in_front)
        .add_observer(on_enemy_added)
        .add_systems(
            Update,
            (
                initialize_vat_enemy_resources
                    .run_if(not(resource_exists::<VatEnemyState>).and(in_state(Screen::Gameplay))),
                animate_enemies
                    .in_set(PostPhysicsAppSystems::PlayAnimations)
                    .run_if(in_state(Screen::Gameplay)),
            ),
        )
        .add_systems(
            PostUpdate,
            (
                bevy_open_vat::system::update_anim_controller,
                debug_vat_ssbo,
            ),
        );
}

fn debug_vat_ssbo(
    mat_query: Query<&MeshMaterial3d<ExtendedMaterial<StandardMaterial, OpenVatExtension>>>,
    materials: Res<Assets<ExtendedMaterial<StandardMaterial, OpenVatExtension>>>,
    buffers: Res<Assets<ShaderStorageBuffer>>,
    controllers: Query<&VatAnimationController>,
    time: Res<Time>,
    mut timer: Local<f32>,
    mut logged: Local<bool>,
) {
    if *logged {
        return;
    }
    // Wait 5 seconds for everything to stabilize
    *timer += time.delta_secs();
    if *timer < 5.0 {
        return;
    }
    *logged = true;

    let controller_count = controllers.iter().len();
    warn!("VAT SSBO DEBUG: {} controllers", controller_count);

    for mat_handle in mat_query.iter().take(1) {
        if let Some(mat) = materials.get(&mat_handle.0) {
            warn!(
                "  Material found. min_pos={:?} max_pos={:?} frame_count={} y_res={}",
                mat.extension.min_pos,
                mat.extension.max_pos,
                mat.extension.frame_count,
                mat.extension.y_resolution,
            );
            if let Some(buffer) = buffers.get(&mat.extension.instance) {
                match &buffer.data {
                    Some(raw) => {
                        warn!("  SSBO: {} bytes (expect 16 per entry, {} entries)", raw.len(), raw.len() / 16);
                        if raw.len() >= 16 {
                            let sf = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                            let fc = u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]);
                            let rate = f32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]);
                            let ofs = f32::from_le_bytes([raw[12], raw[13], raw[14], raw[15]]);
                            warn!("  Entry[0]: start_frame={sf} frame_count={fc} rate={rate:.4} offset={ofs:.4}");
                        }
                    }
                    None => warn!("  SSBO: data is None (empty buffer)"),
                }
            } else {
                warn!("  SSBO: buffer handle invalid (get returned None)");
            }
        } else {
            warn!("  Material handle invalid");
        }
    }
}

// =============================================================================
// VAT resources — shared across all enemy instances
// =============================================================================

/// Shared VAT rendering resources for all enemy instances, created once on
/// first gameplay frame when all assets are loaded.
#[derive(Resource)]
struct VatEnemyState {
    material: Handle<ExtendedMaterial<StandardMaterial, OpenVatExtension>>,
    next_mesh_tag: u32,
}

/// Links an enemy entity to the child mesh entity that holds the
/// `VatAnimationController`, so `animate_enemies` can update the clip.
#[derive(Component)]
struct VatMeshLink(Entity);

fn initialize_vat_enemy_resources(
    models: Res<Models>,
    images: Res<Assets<Image>>,
    remap_infos: Res<Assets<RemapInfo>>,
    mut vat_materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, OpenVatExtension>>>,
    mut buffers: ResMut<Assets<ShaderStorageBuffer>>,
    mut commands: Commands,
) {
    let Some(remap_info) = remap_infos.get(&models.enemy_remap_info) else {
        return;
    };
    let Some(image) = images.get(&models.enemy_vat_texture) else {
        return;
    };

    let y_resolution = image.texture_descriptor.size.height as f32;
    let buffer = buffers.add(ShaderStorageBuffer::from(&Vec::<VatInstanceData>::new()));

    let material = vat_materials.add(ExtendedMaterial {
        base: StandardMaterial {
            base_color: crate::ui::colors::HEALTH_RED,
            ..default()
        },
        extension: OpenVatExtension {
            vat_texture: models.enemy_vat_texture.clone(),
            min_pos: remap_info.os_remap.min.into(),
            frame_count: remap_info.os_remap.frames,
            max_pos: remap_info.os_remap.max.into(),
            y_resolution,
            instance: buffer,
            ..Default::default()
        },
    });

    commands.insert_resource(VatEnemyState {
        material,
        next_mesh_tag: 0,
    });
}

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

// =============================================================================
// On<Add, Enemy> — attach VAT model to any Enemy entity
// =============================================================================

fn on_enemy_added(
    on: On<Add, Enemy>,
    models: Res<Models>,
    gltf_assets: Res<Assets<Gltf>>,
    mut commands: Commands,
) {
    let entity = on.entity;

    // Remove capsule mesh if present (reconciler may have added it)
    commands
        .entity(entity)
        .remove::<Mesh3d>()
        .remove::<MeshMaterial3d<StandardMaterial>>();

    // Insert behavior, visibility, and physics (kinematic so the server
    // controls position via interpolation, but avian3d still pushes the
    // player's dynamic body out of the way on collision).
    commands.entity(entity).insert((
        EnemyBehavior::default(),
        InheritedVisibility::default(),
        Collider::capsule(0.5, 1.0),
        RigidBody::Kinematic,
    ));

    let Some(gltf) = gltf_assets.get(&models.enemy_scene) else {
        warn!("Enemy VAT GLB not loaded when enemy spawned");
        return;
    };

    let scene = SceneRoot(gltf.scenes[0].clone());
    commands.entity(entity).with_children(|parent| {
        let mut child = parent.spawn((
            Transform::from_xyz(0.0, -0.15, 0.0)
                .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
            scene,
        ));
        child.observe(prepare_enemy_vat_scene);
    });
}

// =============================================================================
// Scene ready — swap material for VAT extended material
// =============================================================================

fn prepare_enemy_vat_scene(
    on: On<SceneInstanceReady>,
    mut vat_state: Option<ResMut<VatEnemyState>>,
    models: Res<Models>,
    children_q: Query<&Children>,
    mesh_entities: Query<Entity, With<Mesh3d>>,
    parents: Query<&ChildOf>,
    mut commands: Commands,
) {
    let Some(vat_state) = vat_state.as_mut() else {
        warn!("VatEnemyState not ready when enemy scene loaded");
        return;
    };

    let scene_entity = on.entity;

    // Walk up to the Enemy entity (scene entity → enemy entity)
    let enemy_entity = if let Ok(parent) = parents.get(scene_entity) {
        parent.parent()
    } else {
        scene_entity
    };

    // Find mesh entities in the scene subtree and apply VAT material + controller
    apply_vat_to_descendants(
        scene_entity,
        &children_q,
        &mesh_entities,
        &mut commands,
        vat_state,
        &models,
        enemy_entity,
    );
}

fn apply_vat_to_descendants(
    entity: Entity,
    children_q: &Query<&Children>,
    mesh_entities: &Query<Entity, With<Mesh3d>>,
    commands: &mut Commands,
    vat_state: &mut VatEnemyState,
    models: &Models,
    enemy_entity: Entity,
) {
    if mesh_entities.get(entity).is_ok() {
        let tag = vat_state.next_mesh_tag;
        vat_state.next_mesh_tag += 1;

        commands
            .entity(entity)
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .insert((
                MeshMaterial3d(vat_state.material.clone()),
                VatAnimationController {
                    remap_info: models.enemy_remap_info.clone(),
                    current_clip: "Zombie_Idle_Loop".to_string(),
                    speed: 1.0,
                    is_playing: true,
                    ..Default::default()
                },
                MeshTag(tag),
            ));

        commands.entity(enemy_entity).insert(VatMeshLink(entity));
    }

    if let Ok(children) = children_q.get(entity) {
        for child in children.iter() {
            apply_vat_to_descendants(
                child,
                children_q,
                mesh_entities,
                commands,
                vat_state,
                models,
                enemy_entity,
            );
        }
    }
}

// =============================================================================
// Animation driver — maps EnemyBehavior to VAT clip names
// =============================================================================

fn animate_enemies(
    enemies: Query<(&EnemyBehavior, &VatMeshLink), Changed<EnemyBehavior>>,
    mut controllers: Query<&mut VatAnimationController>,
) {
    for (behavior, vat_link) in &enemies {
        let Ok(mut controller) = controllers.get_mut(vat_link.0) else {
            continue;
        };

        let clip_name = match behavior {
            EnemyBehavior::Idle => "Zombie_Idle_Loop",
            EnemyBehavior::Chase => "Zombie_Walk_Fwd_Loop",
            EnemyBehavior::Attack => "Zombie_Scratch",
        };

        if controller.current_clip != clip_name {
            controller.current_clip = clip_name.to_string();
        }
    }
}
