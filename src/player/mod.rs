use crate::combat::{AttackState, Combatant, Health, PlayerCombatant};
use crate::rule_presets::{self, StackingConfig};
use crate::rules::{Effect, OnKillRules, Rule, Stat, Stats};
use crate::*;
use avian3d::prelude::*;
use bevy::scene::SceneInstanceReady;
use bevy_enhanced_input::prelude::*;
#[cfg(feature = "third_person")]
use bevy_third_person_camera::*;
use bevy_tnua::prelude::*;
use bevy_tnua::{TnuaAnimatingState, control_helpers::TnuaSimpleAirActionsCounter};
use bevy_tnua_avian3d::*;
use std::time::Duration;

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

    #[cfg(feature = "third_person")]
    app.add_plugins(ThirdPersonCameraPlugin).configure_sets(
        PostUpdate,
        bevy_third_person_camera::CameraSyncSet.before(TransformSystems::Propagate),
    );

    app.add_systems(OnEnter(Screen::Gameplay), spawn_player)
        .add_systems(
            Update,
            animating
                .in_set(TnuaUserControlsSystems)
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
            DespawnOnExit(Screen::Gameplay),
            pos,
            player,
            // camera target component
            #[cfg(feature = "third_person")]
            ThirdPersonCameraTarget,
            PlayerCtx,
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
            // rules system - base stats + presets
            Stats::new()
                // Core stats
                .with(Stat::MaxHealth, 100.0)
                .with(Stat::Health, 100.0)
                // Attack parameters (read by on_attack_connect)
                .with(Stat::AttackDamage, 25.0)
                .with(Stat::Knockback, 3.0)
                .with(Stat::AttackRange, 3.6)
                .with(Stat::AttackArc, 150.0)
                // Crit stats (used by crit preset)
                .with(Stat::CritChance, 0.20)
                .with(Stat::CritMultiplier, 2.5),
            rule_presets::crit(),
            rule_presets::stacking(StackingConfig::default()),
            // On kill: log for now (not part of a preset yet)
            OnKillRules(vec![
                Rule::new().then(Effect::Log("Enemy killed!".into())),
            ]),
        ))
        // spawn character mesh as child to adjust mesh position relative to the player origin
        .with_children(|parent| {
            let mut e = parent.spawn((Transform::from_xyz(0.0, -1.0, 0.0), mesh));
            e.observe(prepare_animations);

            // DEBUG
            let collider_mesh = Mesh::from(Capsule3d::new(
                cfg.player.hitbox.radius,
                cfg.player.hitbox.height,
            ));
            let debug_collider_mesh = Mesh3d(meshes.add(collider_mesh.clone()));
            let debug_collider_color: MeshMaterial3d<StandardMaterial> =
                MeshMaterial3d(materials.add(Color::srgba(0.9, 0.9, 0.9, 0.1)));
            parent.spawn((
                debug_collider_mesh,
                debug_collider_color,
                Transform::from_xyz(0.0, -0.1, 0.0),
            ));
            // DEBUG
        });

    Ok(())
}

fn player_post_spawn(on: On<Add, Player>, mut players: Query<&mut Player>) {
    if let Ok(mut p) = players.get_mut(on.entity) {
        p.id = on.entity; // update player id with spawned entity
        // info!("player entity: Player.id: {}", p.id);
    }
}
