use std::time::Duration;

use bevy::input::common_conditions::input_just_pressed;
use bevy::pbr::ExtendedMaterial;
use bevy::prelude::*;
use bevy_open_vat::prelude::OpenVatExtension;

use crate::combat::EnemyBehavior;
use crate::combat::enemy::{VatEnemyState, VatMeshLink};
use crate::ui::colors;

type VatMaterial = ExtendedMaterial<StandardMaterial, OpenVatExtension>;

const WARMUP_DURATION: Duration = Duration::from_secs(1);
const RECORD_DURATION: Duration = Duration::from_secs(4);

// ── Plugin ──────────────────────────────────────────────────────────────

pub fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        toggle_profiler.run_if(input_just_pressed(KeyCode::F10)),
    );
    app.add_systems(
        Update,
        tick_profiler.run_if(resource_exists::<GpuProfilerState>),
    );
}

// ── Phase trait ─────────────────────────────────────────────────────────

trait ProfilePhase: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn apply(&self, world: &mut World);
    fn revert(&self, world: &mut World);
}

// ── Phase implementations ───────────────────────────────────────────────

/// Throwaway first phase — lets the GPU thermally settle before real measurements.
struct Warmup;
impl ProfilePhase for Warmup {
    fn name(&self) -> &'static str { "Warmup" }
    fn apply(&self, _world: &mut World) {}
    fn revert(&self, _world: &mut World) {}
}

struct Baseline;
impl ProfilePhase for Baseline {
    fn name(&self) -> &'static str { "Baseline" }
    fn apply(&self, _world: &mut World) {}
    fn revert(&self, _world: &mut World) {}
}

struct ShadowsOff;
impl ProfilePhase for ShadowsOff {
    fn name(&self) -> &'static str { "Shadows Off" }

    fn apply(&self, world: &mut World) {
        let mut query = world.query::<&mut DirectionalLight>();
        for mut light in query.iter_mut(world) {
            light.shadows_enabled = false;
        }
    }

    fn revert(&self, world: &mut World) {
        let mut query = world.query::<&mut DirectionalLight>();
        for mut light in query.iter_mut(world) {
            light.shadows_enabled = true;
        }
    }
}

struct EnemiesHidden;
impl ProfilePhase for EnemiesHidden {
    fn name(&self) -> &'static str { "Enemies Hidden" }

    fn apply(&self, world: &mut World) {
        // Hide mesh children (not parent Enemy entities) to avoid
        // cull_enemies_beyond_fog overriding visibility every frame.
        let mut links: Vec<Entity> = Vec::new();
        {
            let mut query = world.query_filtered::<&VatMeshLink, With<EnemyBehavior>>();
            for link in query.iter(world) {
                links.push(link.0);
            }
        }
        for mesh_entity in links {
            if let Ok(mut entity_mut) = world.get_entity_mut(mesh_entity) {
                entity_mut.insert(Visibility::Hidden);
            }
        }
    }

    fn revert(&self, world: &mut World) {
        let mut links: Vec<Entity> = Vec::new();
        {
            let mut query = world.query_filtered::<&VatMeshLink, With<EnemyBehavior>>();
            for link in query.iter(world) {
                links.push(link.0);
            }
        }
        for mesh_entity in links {
            if let Ok(mut entity_mut) = world.get_entity_mut(mesh_entity) {
                entity_mut.insert(Visibility::Inherited);
            }
        }
    }
}

struct UnlitEnemies {
    unlit_material: Handle<VatMaterial>,
    original_material: Handle<VatMaterial>,
}

impl ProfilePhase for UnlitEnemies {
    fn name(&self) -> &'static str { "Unlit Enemies" }

    fn apply(&self, world: &mut World) {
        let mut links: Vec<Entity> = Vec::new();
        {
            let mut query = world.query_filtered::<&VatMeshLink, With<EnemyBehavior>>();
            for link in query.iter(world) {
                links.push(link.0);
            }
        }
        for mesh_entity in links {
            if let Ok(mut entity_mut) = world.get_entity_mut(mesh_entity) {
                entity_mut.insert(MeshMaterial3d(self.unlit_material.clone()));
            }
        }
    }

    fn revert(&self, world: &mut World) {
        let mut links: Vec<Entity> = Vec::new();
        {
            let mut query = world.query_filtered::<&VatMeshLink, With<EnemyBehavior>>();
            for link in query.iter(world) {
                links.push(link.0);
            }
        }
        for mesh_entity in links {
            if let Ok(mut entity_mut) = world.get_entity_mut(mesh_entity) {
                entity_mut.insert(MeshMaterial3d(self.original_material.clone()));
            }
        }
    }
}

// ── State machine ───────────────────────────────────────────────────────

#[derive(PartialEq, Eq)]
enum PhaseStage {
    Warmup,
    Recording,
}

struct PhaseResult {
    name: &'static str,
    avg_ms: f32,
    p50: f32,
    p95: f32,
    avg_fps: f32,
}

#[derive(Resource)]
struct GpuProfilerState {
    phases: Vec<Box<dyn ProfilePhase>>,
    current_idx: usize,
    stage: PhaseStage,
    warmup_elapsed: Duration,
    record_elapsed: Duration,
    frame_times: Vec<f32>,
    results: Vec<PhaseResult>,
}

#[derive(Component)]
struct ProfilerOverlay;

// ── Toggle system ───────────────────────────────────────────────────────

fn toggle_profiler(
    mut commands: Commands,
    existing: Option<Res<GpuProfilerState>>,
    overlay: Query<Entity, With<ProfilerOverlay>>,
    vat_state: Option<Res<VatEnemyState>>,
    vat_materials: Res<Assets<VatMaterial>>,
) {
    if existing.is_some() {
        // Cancel — revert current phase before removing
        commands.queue(|world: &mut World| {
            let state = world.remove_resource::<GpuProfilerState>().unwrap();
            if state.current_idx < state.phases.len() {
                state.phases[state.current_idx].revert(world);
            }
        });
        for entity in &overlay {
            commands.entity(entity).despawn();
        }
        info!("GPU profiler cancelled.");
        return;
    }

    // Build unlit material by cloning the enemy material with unlit: true
    let Some(vat_state) = vat_state else {
        warn!("GPU profiler: VatEnemyState not ready — spawn enemies first.");
        return;
    };

    let original_handle = vat_state.material.clone();
    let Some(original_mat) = vat_materials.get(&original_handle) else {
        warn!("GPU profiler: enemy material not loaded.");
        return;
    };

    let unlit_mat = ExtendedMaterial {
        base: StandardMaterial {
            unlit: true,
            ..original_mat.base.clone()
        },
        extension: original_mat.extension.clone(),
    };

    // Need to add the material through commands since we don't have ResMut<Assets<VatMaterial>>
    // Instead, we'll queue a command that creates it
    let original_for_phase = original_handle.clone();
    let phases_data = (unlit_mat, original_for_phase);

    commands.queue(move |world: &mut World| {
        let unlit_handle = world
            .resource_mut::<Assets<VatMaterial>>()
            .add(phases_data.0);

        let phases: Vec<Box<dyn ProfilePhase>> = vec![
            Box::new(Warmup),
            Box::new(ShadowsOff),
            Box::new(EnemiesHidden),
            Box::new(UnlitEnemies {
                unlit_material: unlit_handle,
                original_material: phases_data.1,
            }),
            Box::new(Baseline),
        ];

        let num_phases = phases.len();
        let total_secs = num_phases as u64 * (WARMUP_DURATION + RECORD_DURATION).as_secs();

        // Apply first phase (Warmup is a no-op)
        phases[0].apply(world);

        world.insert_resource(GpuProfilerState {
            phases,
            current_idx: 0,
            stage: PhaseStage::Warmup,
            warmup_elapsed: Duration::ZERO,
            record_elapsed: Duration::ZERO,
            frame_times: Vec::with_capacity(1024),
            results: Vec::new(),
        });

        // Spawn overlay
        world.spawn((
            ProfilerOverlay,
            Text::new(format!("GPU PROFILE  ~{}s", total_secs)),
            TextFont {
                font_size: 18.0,
                ..default()
            },
            TextColor(colors::ACID_GREEN),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(16.0),
                right: Val::Px(16.0),
                ..default()
            },
        ));

        info!(
            "GPU profiler started — {} phases, ~{}s total (F10 to cancel)...",
            num_phases - 1, total_secs,
        );
    });
}

// ── Tick system ─────────────────────────────────────────────────────────

fn tick_profiler(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut state: ResMut<GpuProfilerState>,
    mut overlay: Query<&mut Text, With<ProfilerOverlay>>,
    overlay_entities: Query<Entity, With<ProfilerOverlay>>,
) {
    let delta = time.delta();
    let phase_name = state.phases[state.current_idx].name();

    match state.stage {
        PhaseStage::Warmup => {
            state.warmup_elapsed += delta;
            if state.warmup_elapsed >= WARMUP_DURATION {
                state.stage = PhaseStage::Recording;
                state.record_elapsed = Duration::ZERO;
                state.frame_times.clear();
            }
        }
        PhaseStage::Recording => {
            state.record_elapsed += delta;
            state.frame_times.push(delta.as_secs_f32() * 1000.0);

            if state.record_elapsed >= RECORD_DURATION {
                // Compute result for this phase
                let result = compute_result(phase_name, &state.frame_times);
                state.results.push(result);

                // Revert current phase and advance
                let current_idx = state.current_idx;
                let next_idx = current_idx + 1;

                // We need to revert via command since phases need &mut World
                if next_idx < state.phases.len() {
                    state.current_idx = next_idx;
                    state.stage = PhaseStage::Warmup;
                    state.warmup_elapsed = Duration::ZERO;
                    state.frame_times.clear();

                    commands.queue(move |world: &mut World| {
                        let state = world.remove_resource::<GpuProfilerState>().unwrap();
                        state.phases[current_idx].revert(world);
                        state.phases[next_idx].apply(world);
                        world.insert_resource(state);
                    });
                } else {
                    // All phases done — print summary and clean up
                    let results = std::mem::take(&mut state.results);
                    commands.queue(move |world: &mut World| {
                        let state = world.remove_resource::<GpuProfilerState>().unwrap();
                        state.phases[current_idx].revert(world);
                        log_summary(&results);
                    });
                    for entity in &overlay_entities {
                        commands.entity(entity).despawn();
                    }
                    return;
                }
            }
        }
    }

    // Update overlay text
    let remaining_this_phase = match state.stage {
        PhaseStage::Warmup => {
            WARMUP_DURATION.saturating_sub(state.warmup_elapsed) + RECORD_DURATION
        }
        PhaseStage::Recording => RECORD_DURATION.saturating_sub(state.record_elapsed),
    };
    let phases_left = state.phases.len() - state.current_idx - 1;
    let per_phase = WARMUP_DURATION + RECORD_DURATION;
    let total_remaining =
        remaining_this_phase + per_phase * phases_left as u32;

    for mut text in &mut overlay {
        text.0 = format!(
            "GPU PROFILE  [{}]  {:.0}s",
            state.phases[state.current_idx].name(),
            total_remaining.as_secs_f32().ceil(),
        );
    }
}

// ── Stats computation ───────────────────────────────────────────────────

fn compute_result(name: &'static str, frame_times: &[f32]) -> PhaseResult {
    let count = frame_times.len();
    if count == 0 {
        return PhaseResult {
            name,
            avg_ms: 0.0,
            p50: 0.0,
            p95: 0.0,
            avg_fps: 0.0,
        };
    }

    let mut sorted = frame_times.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let sum: f32 = sorted.iter().sum();
    let avg_ms = sum / count as f32;
    let avg_fps = 1000.0 / avg_ms;
    let p50 = sorted[count * 50 / 100];
    let p95 = sorted[(count * 95 / 100).min(count - 1)];

    PhaseResult {
        name,
        avg_ms,
        p50,
        p95,
        avg_fps,
    }
}

// ── Summary logging ─────────────────────────────────────────────────────

fn log_summary(results: &[PhaseResult]) {
    let mut report = String::from("\n=== GPU PROFILE ===\n");
    report.push_str(&format!(
        "{:<22} {:>8} {:>8} {:>8} {:>8}\n",
        "Phase", "Avg ms", "p50", "p95", "Avg FPS"
    ));

    for r in results.iter().filter(|r| r.name != "Warmup") {
        report.push_str(&format!(
            "{:<22} {:>8.1} {:>8.1} {:>8.1} {:>8.1}\n",
            r.name, r.avg_ms, r.p50, r.p95, r.avg_fps,
        ));
    }

    let find = |name: &str| results.iter().find(|r| r.name == name).map(|r| r.avg_ms);
    if let (Some(baseline), Some(shadows_off), Some(enemies_hidden), Some(unlit_enemies)) = (
        find("Baseline"),
        find("Shadows Off"),
        find("Enemies Hidden"),
        find("Unlit Enemies"),
    ) {

        let shadow_cost = baseline - shadows_off;
        let total_enemy_cost = baseline - enemies_hidden;
        let pbr_frag_cost = baseline - unlit_enemies;
        let vertex_prepass_cost = unlit_enemies - enemies_hidden;

        let pct = |cost: f32| {
            if baseline > 0.0 {
                cost / baseline * 100.0
            } else {
                0.0
            }
        };

        report.push_str("\n=== COST BREAKDOWN ===\n");
        report.push_str(&format!(
            "Shadow maps:            {:>5.1} ms  ({:>4.1}%)\n",
            shadow_cost,
            pct(shadow_cost),
        ));
        report.push_str(&format!(
            "Total enemy rendering:  {:>5.1} ms  ({:>4.1}%)\n",
            total_enemy_cost,
            pct(total_enemy_cost),
        ));
        report.push_str(&format!(
            "  PBR fragment shading: {:>5.1} ms  ({:>4.1}%)\n",
            pbr_frag_cost,
            pct(pbr_frag_cost),
        ));
        report.push_str(&format!(
            "  Vertex + prepass:     {:>5.1} ms  ({:>4.1}%)\n",
            vertex_prepass_cost,
            pct(vertex_prepass_cost),
        ));

    }

    info!("{report}");
}
