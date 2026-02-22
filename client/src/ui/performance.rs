use super::*;
use crate::networking::PingTracker;
use crate::networking::diagnostics::ServerDiagnostics;
use bevy::diagnostic::{DiagnosticsStore, EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin};
use bevy::input::common_conditions::input_just_pressed;
use std::time::Duration;
use web_time::Instant;

const REFRESH_INTERVAL: Duration = Duration::from_millis(200);
const BENCHMARK_DURATION: Duration = Duration::from_secs(10);

// ── Plugin ───────────────────────────────────────────────────────────────

pub fn plugin(app: &mut App) {
    app.insert_resource(StatsTimer(Timer::new(
        REFRESH_INTERVAL,
        TimerMode::Repeating,
    )));
    app.init_resource::<CpuFrameTimer>();
    app.add_systems(First, mark_frame_start);
    app.add_systems(Last, mark_frame_end);
    app.add_systems(
        OnEnter(crate::models::Screen::Gameplay),
        spawn_stats_overlay,
    );
    app.add_systems(
        Update,
        tick_stats_overlay.run_if(in_state(crate::models::Screen::Gameplay)),
    );
    app.add_systems(
        Update,
        toggle_benchmark.run_if(input_just_pressed(KeyCode::F9)),
    );
    app.add_systems(
        Update,
        tick_benchmark.run_if(resource_exists::<BenchmarkFrames>),
    );
}

// ── CPU time tracking ───────────────────────────────────────────────────

#[derive(Resource)]
struct CpuFrameTimer {
    frame_start: Option<Instant>,
    /// Smoothed CPU time in ms (exponential moving average)
    cpu_ms: f64,
}

impl Default for CpuFrameTimer {
    fn default() -> Self {
        Self {
            frame_start: None,
            cpu_ms: 0.0,
        }
    }
}

fn mark_frame_start(mut timer: ResMut<CpuFrameTimer>) {
    timer.frame_start = Some(Instant::now());
}

fn mark_frame_end(mut timer: ResMut<CpuFrameTimer>) {
    if let Some(start) = timer.frame_start.take() {
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        // EMA with ~10-frame smoothing
        timer.cpu_ms = timer.cpu_ms * 0.9 + elapsed_ms * 0.1;
    }
}

// ── Stats overlay ────────────────────────────────────────────────────────

#[derive(Component)]
struct StatsOverlayText;

#[derive(Resource)]
struct StatsTimer(Timer);

fn spawn_stats_overlay(mut commands: Commands) {
    commands.spawn((
        StatsOverlayText,
        Text::new(""),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextColor(colors::NEUTRAL400),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(16.0),
            top: Val::Px(16.0),
            ..default()
        },
        GlobalZIndex(i32::MAX - 32),
        Pickable::IGNORE,
    ));
}

fn tick_stats_overlay(
    time: Res<Time<Real>>,
    mut timer: ResMut<StatsTimer>,
    diag: Option<Res<ServerDiagnostics>>,
    ping: Option<Res<PingTracker>>,
    diagnostics: Res<DiagnosticsStore>,
    cpu_timer: Res<CpuFrameTimer>,
    mut texts: Query<&mut Text, With<StatsOverlayText>>,
) {
    timer.0.tick(time.delta());
    if !timer.0.just_finished() {
        return;
    }

    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    let frame_ms = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    let entity_count = diagnostics
        .get(&EntityCountDiagnosticsPlugin::ENTITY_COUNT)
        .and_then(|d| d.value())
        .unwrap_or(0.0) as u32;

    let cpu_ms = cpu_timer.cpu_ms;
    let enemies = diag.as_ref().map(|d| d.enemy_alive).unwrap_or(0);
    let players = diag.as_ref().map(|d| d.players.len()).unwrap_or(0);
    let ping_ms = ping.as_ref().map(|p| p.smoothed_rtt_ms).unwrap_or(0.0);

    // If CPU time is >80% of frame time, we're CPU-bound
    let bottleneck = if frame_ms > 0.1 {
        if cpu_ms / frame_ms > 0.80 {
            "CPU"
        } else {
            "GPU"
        }
    } else {
        "—"
    };

    let mut line = format!(
        "{fps:.0} FPS  {frame_ms:.1}ms (cpu {cpu_ms:.1}ms) [{bottleneck}]  |  {entity_count} ent  {enemies} enemies  {players} players"
    );
    if ping_ms > 0.0 {
        line.push_str(&format!("  |  {ping_ms:.0} ms"));
    }

    if let Ok(mut text) = texts.single_mut() {
        if text.0 != line {
            text.0 = line;
        }
    }
}

// ── ECS resources ────────────────────────────────────────────────────────

#[derive(Resource)]
struct BenchmarkFrames {
    frame_times: Vec<f32>,
    elapsed: Duration,
}

#[derive(Component)]
struct BenchmarkOverlay;

// ── Benchmark systems ────────────────────────────────────────────────────

fn toggle_benchmark(
    mut commands: Commands,
    existing: Option<Res<BenchmarkFrames>>,
    overlay: Query<Entity, With<BenchmarkOverlay>>,
) {
    if existing.is_some() {
        let _ = crate::profiling::stop();
        commands.remove_resource::<BenchmarkFrames>();
        for entity in &overlay {
            commands.entity(entity).despawn();
        }
        info!("Benchmark cancelled.");
    } else {
        crate::profiling::start();
        commands.insert_resource(BenchmarkFrames {
            frame_times: Vec::with_capacity(1024),
            elapsed: Duration::ZERO,
        });
        commands.spawn((
            BenchmarkOverlay,
            Text::new(format!("BENCHMARK  {}s", BENCHMARK_DURATION.as_secs())),
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
            "Benchmark started — recording for {}s (F9 to cancel)...",
            BENCHMARK_DURATION.as_secs()
        );
    }
}

fn tick_benchmark(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut frames: ResMut<BenchmarkFrames>,
    entities: Query<Entity>,
    mut overlay: Query<&mut Text, With<BenchmarkOverlay>>,
    overlay_entities: Query<Entity, With<BenchmarkOverlay>>,
) {
    let delta = time.delta();
    frames.elapsed += delta;
    frames.frame_times.push(delta.as_secs_f32() * 1000.0);

    let remaining = BENCHMARK_DURATION.saturating_sub(frames.elapsed);
    for mut text in &mut overlay {
        text.0 = format!("BENCHMARK  {:.0}s", remaining.as_secs_f32().ceil());
    }

    if frames.elapsed >= BENCHMARK_DURATION {
        let entity_count = entities.iter().count();
        let report = build_report(&frames.frame_times, entity_count);
        let system_timings = crate::profiling::stop();
        let system_report = crate::profiling::format_report(&system_timings, frames.elapsed);

        commands.remove_resource::<BenchmarkFrames>();
        for entity in &overlay_entities {
            commands.entity(entity).despawn();
        }

        info!("\n{report}\n{system_report}");
    }
}

// ── Report generation ────────────────────────────────────────────────────

fn build_report(frame_times: &[f32], entity_count: usize) -> String {
    let count = frame_times.len();
    if count == 0 {
        return "No frames recorded.".to_string();
    }

    let mut sorted = frame_times.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let sum: f32 = sorted.iter().sum();
    let avg_ms = sum / count as f32;
    let avg_fps = 1000.0 / avg_ms;

    let p50 = sorted[count * 50 / 100];
    let p95 = sorted[count * 95 / 100];
    let p99 = sorted[count * 99 / 100];

    let worst_1 = (count / 100).max(1);
    let low_1_ms: f32 = sorted[count - worst_1..].iter().sum::<f32>() / worst_1 as f32;
    let low_1_fps = 1000.0 / low_1_ms;

    let worst_01 = (count / 1000).max(1);
    let low_01_ms: f32 = sorted[count - worst_01..].iter().sum::<f32>() / worst_01 as f32;
    let low_01_fps = 1000.0 / low_01_ms;

    format!(
        "\
=== FRAME TIMING ===
Frames: {count}  |  Entities: {entity_count}  |  Duration: {:.1}s
Avg FPS: {avg_fps:.1}  |  1% low: {low_1_fps:.1}  |  0.1% low: {low_01_fps:.1}
Frame time (ms):  avg={avg_ms:.2}  p50={p50:.2}  p95={p95:.2}  p99={p99:.2}",
        sum / 1000.0
    )
}
