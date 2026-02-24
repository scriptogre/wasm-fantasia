//! GPU pass profiler using wgpu timestamp queries.
//!
//! Press F10 to start recording. After 10 seconds, prints a per-pass GPU timing
//! report to the terminal. Press F10 again to cancel early.

use std::sync::{
    mpsc::{self, Receiver, Sender},
    Mutex,
};

use bevy::core_pipeline::core_3d::graph::{Core3d, Node3d};
use bevy::input::common_conditions::input_just_pressed;
use bevy::pbr::graph::NodePbr;
use bevy::prelude::*;
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::render_graph::{
    Node as RenderNode, NodeRunError, RenderGraphContext, RenderLabel,
};
use bevy::render::render_resource::{BufferDescriptor, BufferUsages, MapMode, PollType};
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue};
use bevy::render::{Render, RenderApp, RenderStartup, RenderSystems};

use crate::ui::colors;

// ── Constants ───────────────────────────────────────────────────────────

/// 6 spans × 2 timestamps each = 12 slots.
const NUM_TIMESTAMP_SLOTS: u32 = 12;
const NUM_SPANS: usize = 6;
const RECORD_SECONDS: f32 = 10.0;

const SPAN_NAMES: [&str; NUM_SPANS] = [
    "shadow",
    "prepass",
    "main_pass",
    "post_processing",
    "gpu_preprocess",
    "full_frame",
];

// ── Render labels ───────────────────────────────────────────────────────

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
enum TimestampLabel {
    BeforeShadow,
    AfterShadow,
    BeforePrepass,
    AfterPrepass,
    BeforeMainPass,
    AfterMainPass,
    BeforePostProcess,
    AfterPostProcess,
    BeforeGpuPreprocess,
    AfterGpuPreprocess,
    FrameStart,
    FrameEnd,
    Resolve,
}

// ── Timestamp write node ────────────────────────────────────────────────

struct TimestampNode {
    slot: u32,
}

impl TimestampNode {
    fn new(slot: u32) -> Self {
        Self { slot }
    }
}

impl RenderNode for TimestampNode {
    fn run<'w>(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        let Some(state) = world.get_resource::<GpuTimestampState>() else {
            return Ok(());
        };
        if !state.active {
            return Ok(());
        }
        let Some(ref query_set) = state.query_set else {
            return Ok(());
        };

        render_context
            .command_encoder()
            .write_timestamp(query_set, self.slot);

        Ok(())
    }
}

// ── Resolve + copy node ─────────────────────────────────────────────────

struct TimestampResolveNode;

impl RenderNode for TimestampResolveNode {
    fn run<'w>(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        let Some(state) = world.get_resource::<GpuTimestampState>() else {
            return Ok(());
        };
        if !state.active {
            return Ok(());
        }
        let (Some(query_set), Some(resolve_buf)) =
            (&state.query_set, &state.resolve_buffer)
        else {
            return Ok(());
        };

        let encoder = render_context.command_encoder();
        encoder.resolve_query_set(query_set, 0..NUM_TIMESTAMP_SLOTS, resolve_buf, 0);

        // Copy resolve → readback (ping-pong)
        let readback_idx = (state.frame_index % 2) as usize;
        if let Some(ref readback) = state.readback_buffers[readback_idx] {
            let size = (NUM_TIMESTAMP_SLOTS as u64) * size_of::<u64>() as u64;
            encoder.copy_buffer_to_buffer(resolve_buf, 0, readback, 0, size);
        }

        Ok(())
    }
}

// ── GPU-side state (render world) ───────────────────────────────────────

#[derive(Resource)]
struct GpuTimestampState {
    active: bool,
    query_set: Option<wgpu::QuerySet>,
    resolve_buffer: Option<bevy::render::render_resource::Buffer>,
    readback_buffers: [Option<bevy::render::render_resource::Buffer>; 2],
    timestamp_period: f64,
    frame_index: u64,
    sender: Sender<Vec<f32>>,
}

// ── Main-world control (extracted to render world) ──────────────────────

#[derive(Resource, Clone, ExtractResource)]
struct GpuPassProfilerControl {
    active: bool,
}

// ── Main-world recording state ──────────────────────────────────────────

#[derive(Resource)]
struct GpuPassRecording {
    samples: Vec<[f32; NUM_SPANS]>,
    elapsed: f32,
    frame_count: u32,
}

/// Shared channel receiver, wrapped in Mutex for Sync.
#[derive(Resource)]
struct PassProfilerChannel(Mutex<Receiver<Vec<f32>>>);

#[derive(Component)]
struct PassProfilerOverlay;

// ── Plugin ──────────────────────────────────────────────────────────────

pub fn plugin(app: &mut App) {
    app.add_plugins(ExtractResourcePlugin::<GpuPassProfilerControl>::default());

    app.add_systems(
        Update,
        toggle_pass_profiler.run_if(input_just_pressed(KeyCode::F10)),
    );
    app.add_systems(
        Update,
        tick_pass_profiler.run_if(resource_exists::<GpuPassRecording>),
    );

    let (sender, receiver) = mpsc::channel::<Vec<f32>>();
    app.insert_resource(PassProfilerChannel(Mutex::new(receiver)));

    let render_app = app.sub_app_mut(RenderApp);

    render_app.insert_resource(GpuTimestampState {
        active: false,
        query_set: None,
        resolve_buffer: None,
        readback_buffers: [None, None],
        timestamp_period: 0.0,
        frame_index: 0,
        sender,
    });

    render_app.add_systems(RenderStartup, init_gpu_timestamp_resources);
    render_app.add_systems(
        Render,
        sync_profiler_control.in_set(RenderSystems::ExtractCommands),
    );
    render_app.add_systems(
        Render,
        readback_timestamps.in_set(RenderSystems::Cleanup),
    );

    // Register timestamp nodes directly in the Core3d sub-graph
    let render_world = render_app.world_mut();
    let mut graph = render_world.resource_mut::<bevy::render::render_graph::RenderGraph>();
    let core3d = graph
        .get_sub_graph_mut(Core3d)
        .expect("Core3d sub-graph must exist");

    // Add all timestamp nodes (slot indices match SPAN_NAMES order × 2)
    // shadow: slots 0,1
    core3d.add_node(TimestampLabel::BeforeShadow, TimestampNode::new(0));
    core3d.add_node(TimestampLabel::AfterShadow, TimestampNode::new(1));
    // prepass: slots 2,3
    core3d.add_node(TimestampLabel::BeforePrepass, TimestampNode::new(2));
    core3d.add_node(TimestampLabel::AfterPrepass, TimestampNode::new(3));
    // main_pass: slots 4,5
    core3d.add_node(TimestampLabel::BeforeMainPass, TimestampNode::new(4));
    core3d.add_node(TimestampLabel::AfterMainPass, TimestampNode::new(5));
    // post_processing: slots 6,7
    core3d.add_node(TimestampLabel::BeforePostProcess, TimestampNode::new(6));
    core3d.add_node(TimestampLabel::AfterPostProcess, TimestampNode::new(7));
    // gpu_preprocess: slots 8,9
    core3d.add_node(TimestampLabel::BeforeGpuPreprocess, TimestampNode::new(8));
    core3d.add_node(TimestampLabel::AfterGpuPreprocess, TimestampNode::new(9));
    // full_frame: slots 10,11
    core3d.add_node(TimestampLabel::FrameStart, TimestampNode::new(10));
    core3d.add_node(TimestampLabel::FrameEnd, TimestampNode::new(11));
    // resolve
    core3d.add_node(TimestampLabel::Resolve, TimestampResolveNode);

    // ── Edge wiring ──

    // Shadow span: before EarlyShadowPass → after LateShadowPass
    core3d.add_node_edge(TimestampLabel::BeforeShadow, NodePbr::EarlyShadowPass);
    core3d.add_node_edge(NodePbr::LateShadowPass, TimestampLabel::AfterShadow);

    // Prepass span: before EarlyPrepass → after EndPrepasses
    core3d.add_node_edge(TimestampLabel::BeforePrepass, Node3d::EarlyPrepass);
    core3d.add_node_edge(Node3d::EndPrepasses, TimestampLabel::AfterPrepass);

    // Main pass span: before StartMainPass → after EndMainPass
    core3d.add_node_edge(TimestampLabel::BeforeMainPass, Node3d::StartMainPass);
    core3d.add_node_edge(Node3d::EndMainPass, TimestampLabel::AfterMainPass);

    // Post-processing span
    core3d.add_node_edge(
        TimestampLabel::BeforePostProcess,
        Node3d::StartMainPassPostProcessing,
    );
    core3d.add_node_edge(
        Node3d::EndMainPassPostProcessing,
        TimestampLabel::AfterPostProcess,
    );

    // GPU preprocess span: before EarlyGpuPreprocess → after LateGpuPreprocess
    core3d.add_node_edge(
        TimestampLabel::BeforeGpuPreprocess,
        NodePbr::EarlyGpuPreprocess,
    );
    core3d.add_node_edge(
        NodePbr::LateGpuPreprocess,
        TimestampLabel::AfterGpuPreprocess,
    );

    // Full frame: FrameStart before everything, FrameEnd after Upscaling
    core3d.add_node_edge(TimestampLabel::FrameStart, TimestampLabel::BeforeGpuPreprocess);
    core3d.add_node_edge(TimestampLabel::FrameStart, TimestampLabel::BeforeShadow);
    core3d.add_node_edge(TimestampLabel::FrameStart, TimestampLabel::BeforePrepass);
    core3d.add_node_edge(Node3d::Upscaling, TimestampLabel::FrameEnd);

    // Ordering edges to prevent ambiguity
    core3d.add_node_edge(TimestampLabel::AfterShadow, Node3d::StartMainPass);
    core3d.add_node_edge(TimestampLabel::AfterPrepass, Node3d::StartMainPass);
    core3d.add_node_edge(TimestampLabel::AfterGpuPreprocess, Node3d::StartMainPass);
    core3d.add_node_edge(
        TimestampLabel::AfterMainPass,
        Node3d::StartMainPassPostProcessing,
    );
    core3d.add_node_edge(TimestampLabel::AfterPostProcess, Node3d::Upscaling);

    // Resolve must be last
    core3d.add_node_edge(TimestampLabel::FrameEnd, TimestampLabel::Resolve);
}

// ── GPU init (render world, runs once) ──────────────────────────────────

fn init_gpu_timestamp_resources(
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    mut state: ResMut<GpuTimestampState>,
) {
    let device = render_device.wgpu_device();

    if !device
        .features()
        .contains(wgpu::Features::TIMESTAMP_QUERY)
    {
        warn!("GPU pass profiler: TIMESTAMP_QUERY not supported — profiler disabled.");
        return;
    }

    let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
        label: Some("gpu_pass_profiler_query_set"),
        ty: wgpu::QueryType::Timestamp,
        count: NUM_TIMESTAMP_SLOTS,
    });

    let buf_size = (NUM_TIMESTAMP_SLOTS as u64) * size_of::<u64>() as u64;

    let resolve_buffer = render_device.create_buffer(&BufferDescriptor {
        label: Some("gpu_pass_profiler_resolve"),
        size: buf_size,
        usage: BufferUsages::QUERY_RESOLVE | BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let readback_a = render_device.create_buffer(&BufferDescriptor {
        label: Some("gpu_pass_profiler_readback_a"),
        size: buf_size,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let readback_b = render_device.create_buffer(&BufferDescriptor {
        label: Some("gpu_pass_profiler_readback_b"),
        size: buf_size,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let timestamp_period = render_queue.get_timestamp_period() as f64; // ns per tick

    state.query_set = Some(query_set);
    state.resolve_buffer = Some(resolve_buffer);
    state.readback_buffers = [Some(readback_a), Some(readback_b)];
    state.timestamp_period = timestamp_period;

    info!(
        "GPU pass profiler initialized (timestamp period: {:.2} ns/tick)",
        timestamp_period
    );
}

// ── Control sync (render world) ─────────────────────────────────────────

fn sync_profiler_control(
    control: Option<Res<GpuPassProfilerControl>>,
    mut state: ResMut<GpuTimestampState>,
) {
    let active = control.is_some_and(|c| c.active);
    if !active && state.active {
        state.frame_index = 0;
    }
    state.active = active;
}

// ── Readback system (render world) ──────────────────────────────────────

fn readback_timestamps(render_device: Res<RenderDevice>, mut state: ResMut<GpuTimestampState>) {
    if !state.active || state.query_set.is_none() {
        return;
    }

    let frame = state.frame_index;
    state.frame_index += 1;

    if frame < 2 {
        return;
    }

    let readback_idx = ((frame - 2) % 2) as usize;
    let Some(ref readback) = state.readback_buffers[readback_idx] else {
        return;
    };

    let device = render_device.wgpu_device();
    let slice = readback.slice(..);

    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    let _ = device.poll(PollType::wait_indefinitely());

    if let Ok(Ok(())) = rx.recv() {
        let data = slice.get_mapped_range();
        let bytes: &[u8] = &data;

        if bytes.len() >= (NUM_TIMESTAMP_SLOTS as usize) * size_of::<u64>() {
            let period = state.timestamp_period;
            let mut span_ms = Vec::with_capacity(NUM_SPANS);

            for i in 0..NUM_SPANS {
                let start_offset = i * 2 * size_of::<u64>();
                let end_offset = (i * 2 + 1) * size_of::<u64>();
                let start =
                    u64::from_ne_bytes(bytes[start_offset..start_offset + 8].try_into().unwrap());
                let end =
                    u64::from_ne_bytes(bytes[end_offset..end_offset + 8].try_into().unwrap());
                let duration_ms = if end >= start {
                    (end - start) as f64 * period / 1_000_000.0
                } else {
                    0.0
                };
                span_ms.push(duration_ms as f32);
            }

            let _ = state.sender.send(span_ms);
        }

        drop(data);
    }

    readback.unmap();
}

// ── F10 toggle (main world) ─────────────────────────────────────────────

fn toggle_pass_profiler(
    mut commands: Commands,
    existing: Option<Res<GpuPassRecording>>,
    overlay: Query<Entity, With<PassProfilerOverlay>>,
    channel: Option<Res<PassProfilerChannel>>,
) {
    if existing.is_some() {
        commands.remove_resource::<GpuPassRecording>();
        commands.remove_resource::<GpuPassProfilerControl>();
        for entity in &overlay {
            commands.entity(entity).despawn();
        }
        info!("GPU pass profiler cancelled.");
        return;
    }

    // Drain stale data
    if let Some(ref channel) = channel {
        if let Ok(rx) = channel.0.lock() {
            while rx.try_recv().is_ok() {}
        }
    }

    commands.insert_resource(GpuPassProfilerControl { active: true });
    commands.insert_resource(GpuPassRecording {
        samples: Vec::with_capacity(1024),
        elapsed: 0.0,
        frame_count: 0,
    });

    commands.spawn((
        PassProfilerOverlay,
        Text::new("GPU PASS PROFILE  10s"),
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

    info!("GPU pass profiler started (10s, press F10 to cancel)...");
}

// ── Recording tick (main world) ──────────────────────────────────────────

fn tick_pass_profiler(
    mut commands: Commands,
    time: Res<Time<Real>>,
    channel: Option<Res<PassProfilerChannel>>,
    mut recording: ResMut<GpuPassRecording>,
    overlay: Query<Entity, With<PassProfilerOverlay>>,
    mut overlay_text: Query<&mut Text, With<PassProfilerOverlay>>,
) {
    let delta = time.delta_secs();
    recording.elapsed += delta;
    recording.frame_count += 1;

    // Drain samples from channel
    if let Some(ref channel) = channel {
        if let Ok(rx) = channel.0.lock() {
            while let Ok(span_ms) = rx.try_recv() {
                if span_ms.len() == NUM_SPANS {
                    let mut arr = [0.0f32; NUM_SPANS];
                    arr.copy_from_slice(&span_ms);
                    recording.samples.push(arr);
                }
            }
        }
    }

    // Update overlay
    let remaining = (RECORD_SECONDS - recording.elapsed).max(0.0).ceil() as u32;
    for mut text in &mut overlay_text {
        text.0 = format!(
            "GPU PASS PROFILE  {}s  ({} samples)",
            remaining,
            recording.samples.len()
        );
    }

    // Check if done
    if recording.elapsed >= RECORD_SECONDS {
        let samples = std::mem::take(&mut recording.samples);
        let frame_count = recording.frame_count;
        let elapsed = recording.elapsed;

        commands.remove_resource::<GpuPassRecording>();
        commands.remove_resource::<GpuPassProfilerControl>();

        for entity in &overlay {
            commands.entity(entity).despawn();
        }

        log_pass_report(&samples, frame_count, elapsed);
    }
}

// ── Report ──────────────────────────────────────────────────────────────

fn log_pass_report(samples: &[[f32; NUM_SPANS]], frame_count: u32, elapsed: f32) {
    let gpu_sample_count = samples.len();
    if gpu_sample_count == 0 {
        info!("GPU pass profiler: no GPU samples collected. Timestamp queries may not be supported.");
        return;
    }

    let mut report = format!(
        "\n=== GPU PASS TIMING ({:.0}s, {} frames, {} GPU samples) ===\n",
        elapsed, frame_count, gpu_sample_count,
    );
    report.push_str(&format!(
        "{:<28} {:>8} {:>8} {:>8} {:>12}\n",
        "Pass", "Avg ms", "p50", "p95", "% of frame"
    ));

    // Compute stats for each span
    let mut span_stats: Vec<(f32, f32, f32)> = Vec::new();
    for span_idx in 0..NUM_SPANS {
        let mut values: Vec<f32> = samples.iter().map(|s| s[span_idx]).collect();
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let count = values.len();
        let sum: f32 = values.iter().sum();
        let avg = sum / count as f32;
        let p50 = values[count / 2];
        let p95 = values[(count * 95 / 100).min(count - 1)];

        span_stats.push((avg, p50, p95));
    }

    // full_frame is the last span — use it as the denominator for percentages
    let full_frame_avg = span_stats[NUM_SPANS - 1].0;

    // Print individual pass spans (first 4)
    for (i, name) in SPAN_NAMES[..4].iter().enumerate() {
        let (avg, p50, p95) = span_stats[i];
        let pct = if full_frame_avg > 0.0 {
            avg / full_frame_avg * 100.0
        } else {
            0.0
        };
        report.push_str(&format!(
            "{:<28} {:>8.1} {:>8.1} {:>8.1} {:>10.1}%\n",
            name, avg, p50, p95, pct,
        ));
    }

    // Print gpu_preprocess
    {
        let (avg, p50, p95) = span_stats[4];
        let pct = if full_frame_avg > 0.0 {
            avg / full_frame_avg * 100.0
        } else {
            0.0
        };
        report.push_str(&format!(
            "{:<28} {:>8.1} {:>8.1} {:>8.1} {:>10.1}%\n",
            "gpu_preprocess", avg, p50, p95, pct,
        ));
    }

    // Separator + totals
    let pass_sum_avg: f32 = (0..4).map(|i| span_stats[i].0).sum();
    report.push_str(&format!(
        "{:<28} {:>8.1}\n",
        "Sum of 4 passes", pass_sum_avg,
    ));

    let (avg, p50, p95) = span_stats[NUM_SPANS - 1];
    report.push_str(&format!(
        "{:<28} {:>8.1} {:>8.1} {:>8.1}\n",
        "Full GPU frame", avg, p50, p95,
    ));

    let unaccounted = avg - pass_sum_avg;
    report.push_str(&format!(
        "{:<28} {:>8.1}\n",
        "Unaccounted GPU time", unaccounted,
    ));

    let frame_avg_ms = elapsed * 1000.0 / frame_count as f32;
    report.push_str(&format!("{:<28} {:>8.1}\n", "Frame (CPU)", frame_avg_ms));

    info!("{report}");
}
