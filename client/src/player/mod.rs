use crate::combat::{AttackState, Combatant, Health, PlayerCombatant};
use crate::rule_presets;
use crate::rules::{Stat, Stats};
use crate::*;
use avian3d::prelude::*;
use bevy::scene::SceneInstanceReady;
use bevy_enhanced_input::prelude::*;
use bevy_third_person_camera::*;
use bevy_tnua::prelude::*;
use bevy_tnua::{TnuaAnimatingState, control_helpers::TnuaSimpleAirActionsCounter};
use bevy_tnua_avian3d::*;
use std::time::Duration;
use wasm_fantasia_shared::combat::defaults;

mod animation;
pub mod control;
mod sound;

pub use animation::*;

/// This plugin handles player related stuff like movement, shooting
/// Player logic is only active during the State `Screen::Playing`
pub fn plugin(app: &mut App) {
    app.add_plugins((
        TnuaControllerPlugin::new(FixedUpdate),
        TnuaAvian3dPlugin::new(FixedUpdate),
        control::plugin,
        sound::plugin,
    ));

    app.add_plugins(ThirdPersonCameraPlugin).configure_sets(
        PostUpdate,
        bevy_third_person_camera::CameraSyncSet.before(TransformSystems::Propagate),
    );

    app.add_systems(OnEnter(Screen::Gameplay), spawn_player)
        .add_systems(
            Update,
            (
                animating.in_set(TnuaUserControlsSystems),
                sync_debug_colliders,
            )
                .run_if(in_state(Screen::Gameplay)),
        )
        .add_observer(player_post_spawn);
}

pub fn spawn_player(
    cfg: Res<Config>,
    models: Res<Models>,
    gltf_assets: Res<Assets<Gltf>>,
    mut commands: Commands,
    // DEBUG
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) -> Result {
    let Some(gltf) = gltf_assets.get(&models.player) else {
        return Ok(());
    };

    let mesh = SceneRoot(gltf.scenes[0].clone());
    let pos = Vec3::from(cfg.player.spawn_pos);
    let pos = Transform::from_translation(pos);
    let player = Player {
        speed: cfg.player.movement.speed,
        animation_state: AnimationState::StandIdle,
        ..default()
    };
    let collider = Collider::capsule(cfg.player.hitbox.radius, cfg.player.hitbox.height);

    commands
        .spawn((
            pos,
            player,
            ThirdPersonCameraTarget,
            // PlayerCtx is NOT inserted here — sync_gameplay_lock adds it
            // when no BlocksGameplay entities exist and the game isn't paused.
            // tnua character control bundles
            (
                TnuaController::default(),
                // Tnua can fix the rotation, but the character will still get rotated before it can do so.
                // By locking the rotation we can prevent this.
                LockedAxes::ROTATION_LOCKED.unlock_rotation_y(),
                TnuaAnimatingState::<AnimationState>::default(),
                TnuaSimpleAirActionsCounter::default(),
                animation::DashAnimationState::default(),
                animation::AttackAnimationState::default(),
                // A sensor shape is not strictly necessary, but without it we'll get weird results.
                TnuaAvian3dSensorShape(collider.clone()),
            ),
            // physics
            (
                collider,
                RigidBody::Dynamic,
                Friction::ZERO.with_combine_rule(CoefficientCombine::Multiply),
            ),
            // other player related components
            (
                JumpTimer(Timer::from_seconds(cfg.timers.jump, TimerMode::Repeating)),
                StepTimer(Timer::from_seconds(cfg.timers.step, TimerMode::Repeating)),
                InheritedVisibility::default(), // silence the warning because of adding SceneRoot as a child
            ),
            // combat components
            (
                Health::new(100.0),
                AttackState::new(0.15), // Fast attack chaining
                Combatant,
                PlayerCombatant,
            ),
            // rules system - base stats + shared rules
            Stats::new()
                .with(Stat::MaxHealth, defaults::HEALTH)
                .with(Stat::Health, defaults::HEALTH)
                .with(Stat::AttackDamage, defaults::ATTACK_DAMAGE)
                .with(Stat::Knockback, defaults::KNOCKBACK)
                .with(Stat::AttackRange, defaults::ATTACK_RANGE)
                .with(Stat::AttackArc, defaults::ATTACK_ARC)
                .with(Stat::CritChance, defaults::CRIT_CHANCE)
                .with(Stat::CritMultiplier, defaults::CRIT_MULTIPLIER),
            rule_presets::rules_bundle(wasm_fantasia_shared::presets::default_player_rules()),
        ))
        // spawn character mesh as child to adjust mesh position relative to the player origin
        .with_children(|parent| {
            let mut e = parent.spawn((Transform::from_xyz(0.0, -1.0, 0.0), mesh));
            e.observe(prepare_animations);

            let collider_mesh = meshes.add(Capsule3d::new(
                cfg.player.hitbox.radius,
                cfg.player.hitbox.height,
            ));
            parent.spawn((
                DebugCollider,
                Mesh3d(collider_mesh),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: crate::ui::colors::NEUTRAL200.with_alpha(0.1),
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    ..default()
                })),
                Transform::from_xyz(0.0, -0.1, 0.0),
                Visibility::Hidden,
            ));
        });

    Ok(())
}

#[derive(Component)]
struct DebugCollider;

fn sync_debug_colliders(
    state: Res<Session>,
    mut colliders: Query<&mut Visibility, With<DebugCollider>>,
) {
    let vis = if state.debug_ui {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut v in &mut colliders {
        *v = vis;
    }
}

fn player_post_spawn(on: On<Add, Player>, mut players: Query<&mut Player>) {
    if let Ok(mut p) = players.get_mut(on.entity) {
        p.id = on.entity; // update player id with spawned entity
        // info!("player entity: Player.id: {}", p.id);
    }
}
