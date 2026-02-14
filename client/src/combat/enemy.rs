use super::*;
use crate::asset_loading::Models;
use crate::models::SpawnEnemy;
use avian3d::prelude::{Collider, RigidBody};
use bevy::pbr::ExtendedMaterial;
use bevy::render::storage::ShaderStorageBuffer;
use bevy_open_vat::data::VatInstanceData;
use bevy::scene::SceneInstanceReady;
use bevy_enhanced_input::prelude::Start;
use bevy_open_vat::prelude::*;

/// bevy_open_vat's material type with StandardMaterial base.
type VatMaterial = ExtendedMaterial<StandardMaterial, OpenVatExtension>;

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
        );
}

// =============================================================================
// VAT resources — shared across all enemy instances
// =============================================================================

/// Shared VAT rendering resources for all enemy instances, created once on
/// first gameplay frame when all assets are loaded.
#[derive(Resource)]
struct VatEnemyState {
    material: Handle<VatMaterial>,
}

/// Links an enemy entity to the child mesh entity that holds the
/// `VatAnimationController`, so `animate_enemies` can update the clip.
#[derive(Component)]
pub(super) struct VatMeshLink(pub Entity);

fn initialize_vat_enemy_resources(
    models: Res<Models>,
    images: Res<Assets<Image>>,
    remap_infos: Res<Assets<RemapInfo>>,
    mut vat_materials: ResMut<Assets<VatMaterial>>,
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

    // Seed with one zeroed entry so the GPU buffer has non-zero arrayLength.
    // bevy_open_vat's update_instance_data system overwrites this every frame.
    let mut buffer = ShaderStorageBuffer::default();
    buffer.set_data(vec![VatInstanceData::default()]);
    let buffer = buffers.add(buffer);

    let material = vat_materials.add(ExtendedMaterial {
        base: StandardMaterial {
            base_color: Color::srgb(0.816, 0.125, 0.125),
            double_sided: true,
            cull_mode: None,
            // Force forward rendering. The project uses deferred rendering by
            // default, but bevy_open_vat overrides vertex_shader() (forward) and
            // prepass_vertex_shader() (prepass). In deferred mode, opaque meshes
            // render through the G-buffer prepass — which DOES use the prepass
            // vertex shader. However, bevy_open_vat's prepass shader has its own
            // Vertex struct that can conflict with deferred-specific shader_defs
            // (NORMAL_PREPASS_OR_DEFERRED_PREPASS). Forward rendering avoids this
            // issue entirely.
            opaque_render_method: bevy::pbr::OpaqueRendererMethod::Forward,
            ..default()
        },
        extension: OpenVatExtension {
            vat_texture: models.enemy_vat_texture.clone(),
            min_pos: remap_info.os_remap.min.into(),
            frame_count: remap_info.os_remap.frames,
            max_pos: remap_info.os_remap.max.into(),
            y_resolution,
            instance: buffer,
        },
    });

    commands.insert_resource(VatEnemyState { material });
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
            Transform::from_xyz(0.0, -0.85, 0.0)
                .with_scale(Vec3::splat(1.25))
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
    vat_state: Option<Res<VatEnemyState>>,
    models: Res<Models>,
    children_q: Query<&Children>,
    mesh_entities: Query<Entity, With<Mesh3d>>,
    parents: Query<&ChildOf>,
    mut commands: Commands,
) {
    let Some(vat_state) = vat_state.as_ref() else {
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
    vat_state: &VatEnemyState,
    models: &Models,
    enemy_entity: Entity,
) {
    if mesh_entities.get(entity).is_ok() {
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
                    start_time: 0.0,
                    offset: 0.0,
                },
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
    time: Res<Time>,
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
            controller.start_time = time.elapsed_secs();
        }
    }
}
