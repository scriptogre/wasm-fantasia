use super::*;
use bevy::dev_tools::fps_overlay::{FpsOverlayConfig, FpsOverlayPlugin, FrameTimeGraphConfig};
use bevy::input::common_conditions::input_just_pressed;
use std::time::Duration;

const FPS_OVERLAY_ZINDEX: i32 = i32::MAX - 32;
const BENCHMARK_DURATION: Duration = Duration::from_secs(10);

// ── Plugin ───────────────────────────────────────────────────────────────

pub fn plugin(app: &mut App) {
    app.add_plugins(FpsOverlayPlugin {
        config: FpsOverlayConfig {
            text_config: TextFont {
                font_size: 14.0,
                ..default()
            },
            text_color: colors::NEUTRAL400,
            enabled: true,
            refresh_interval: Duration::from_millis(100),
            frame_time_graph_config: FrameTimeGraphConfig {
                enabled: false,
                ..default()
            },
        },
    });

    app.add_systems(PostStartup, (strip_fps_label, adjust_fps_layout));
    app.add_systems(
        Update,
        toggle_benchmark.run_if(input_just_pressed(KeyCode::F9)),
    );
    app.add_systems(
        Update,
        tick_benchmark.run_if(resource_exists::<BenchmarkFrames>),
    );
}

// ── FPS overlay ──────────────────────────────────────────────────────────

fn strip_fps_label(mut texts: Query<&mut Text>) {
    for mut text in &mut texts {
        if text.0.starts_with("FPS:") {
            text.0 = String::new();
        }
    }
}

fn adjust_fps_layout(mut nodes: Query<(&GlobalZIndex, &mut Node)>) {
    for (z, mut node) in &mut nodes {
        if z.0 == FPS_OVERLAY_ZINDEX {
            node.left = Val::Px(16.0);
            node.top = Val::Px(16.0);
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
        commands.remove_resource::<BenchmarkFrames>();
        for entity in &overlay {
            commands.entity(entity).despawn();
        }
        info!("Benchmark cancelled.");
    } else {
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

        commands.remove_resource::<BenchmarkFrames>();
        for entity in &overlay_entities {
            commands.entity(entity).despawn();
        }

        info!("\n{report}");
    }
}

// ── Report generation ────────────────────────────────────────────────────

fn build_report(frame_times: &[f32], entity_count: usize) -> String {
    frame_summary(frame_times, entity_count)
}

fn frame_summary(frame_times: &[f32], entity_count: usize) -> String {
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
