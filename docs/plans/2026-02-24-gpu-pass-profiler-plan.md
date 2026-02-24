# GPU Pass Profiler Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the F10 phase-based GPU profiler with a real per-render-pass GPU timestamp profiler.

**Architecture:** Insert render graph nodes before/after major passes that write wgpu timestamp queries. Async readback via ping-pong buffers (2-frame latency). Main-world state machine handles F10 toggle, 10s recording, and terminal report.

**Tech Stack:** Bevy 0.18 render graph, wgpu `TIMESTAMP_QUERY`, `QuerySet`, `CommandEncoder::write_timestamp`

**Design doc:** `docs/plans/2026-02-24-gpu-pass-profiler-design.md`

---

### Task 1: Request TIMESTAMP_QUERY feature at device creation

**Files:**
- Modify: `client/src/main.rs` (~lines 56-70)

**Step 1: Add TIMESTAMP_QUERY to WgpuSettings for native**

Currently native uses `RenderPlugin::default()`. Change it to request timestamp features:

```rust
#[cfg(not(target_arch = "wasm32"))]
let render = bevy::render::RenderPlugin {
    render_creation: RenderCreation::Automatic(WgpuSettings {
        features: bevy::render::settings::WgpuFeatures::TIMESTAMP_QUERY
            | bevy::render::settings::WgpuFeatures::TIMESTAMP_QUERY_INSIDE_ENCODERS,
        ..default()
    }),
    ..default()
};
```

**Step 2: Verify compilation**

Run: `cargo check`
Expected: compiles clean

**Step 3: Commit**

```
feat: request TIMESTAMP_QUERY wgpu feature for native builds
```

---

### Task 2: Create render-world GPU timestamp infrastructure

**Files:**
- Create: `client/src/gpu_pass_profiler.rs`

This task creates the render-world resources and the timestamp render graph node. The node is generic — parameterized by a slot index so we can reuse one struct for all timestamp points.

**Step 1: Create the file with render-world types**

```rust
use bevy::prelude::*;
use bevy::render::{
    render_graph::{Node, NodeRunError, RenderGraphContext, RenderLabel},
    render_resource::Buffer,
    renderer::{RenderContext, RenderDevice, RenderQueue},
};
use wgpu::{
    BufferDescriptor, BufferUsages, MapMode, QuerySet, QuerySetDescriptor, QueryType,
    QUERY_SET_MAX_QUERIES,
};

/// Number of timestamp slots. 2 per measured span.
/// Spans: shadow, prepass, main_pass, post_processing = 4 spans × 2 = 8 slots.
/// Plus we derive "full frame" from prepass-start to post-end (reuses existing slots).
const NUM_TIMESTAMP_SLOTS: u32 = 8;

/// Size in bytes of the timestamp resolve buffer (u64 per slot).
const RESOLVE_BUFFER_SIZE: u64 = NUM_TIMESTAMP_SLOTS as u64 * 8;

/// Labels for timestamp measurement spans.
pub const SPAN_NAMES: &[&str] = &["shadow", "prepass", "main_pass", "post_processing"];

/// Render graph labels for timestamp nodes.
#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub enum TimestampLabel {
    BeforeShadow,
    AfterShadow,
    BeforePrepass,
    AfterPrepass,
    BeforeMainPass,
    AfterMainPass,
    BeforePostProcess,
    AfterPostProcess,
}

/// Render-world resource holding GPU timestamp query infrastructure.
#[derive(Resource)]
pub struct GpuTimestampState {
    pub query_set: QuerySet,
    pub resolve_buffer: Buffer,
    pub readback_buffers: [Buffer; 2],
    pub frame_index: u64,
    /// Whether profiling is currently active.
    pub active: bool,
    /// Timestamp period in nanoseconds (for converting raw timestamps).
    pub timestamp_period: f32,
}
```

**Step 2: Add the `TimestampNode` struct**

A render graph node that writes a single timestamp at a given slot index:

```rust
/// A render graph node that writes a GPU timestamp at a fixed slot.
pub struct TimestampNode {
    slot: u32,
}

impl TimestampNode {
    pub fn new(slot: u32) -> Self {
        Self { slot }
    }
}

impl Node for TimestampNode {
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

        render_context
            .command_encoder()
            .write_timestamp(&state.query_set, self.slot);

        Ok(())
    }
}
```

**Step 3: Add the resolve-and-copy node**

This node runs after all timestamps are written. It resolves the query set to the resolve buffer, then copies to the current readback buffer:

```rust
/// Render graph label for the resolve node.
#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub struct TimestampResolveLabel;

/// Node that resolves timestamp queries and copies to a mappable readback buffer.
pub struct TimestampResolveNode;

impl Node for TimestampResolveNode {
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

        let encoder = render_context.command_encoder();
        let readback_idx = (state.frame_index % 2) as usize;

        encoder.resolve_query_set(&state.query_set, 0..NUM_TIMESTAMP_SLOTS, &state.resolve_buffer, 0);
        encoder.copy_buffer_to_buffer(
            &state.resolve_buffer,
            0,
            &state.readback_buffers[readback_idx],
            0,
            RESOLVE_BUFFER_SIZE,
        );

        Ok(())
    }
}
```

**Step 4: Verify compilation**

Run: `cargo check`
Expected: compiles clean (file not yet wired into main.rs)

**Step 5: Commit**

```
feat: add render-world GPU timestamp infrastructure
```

---

### Task 3: Register timestamp nodes in the render graph

**Files:**
- Modify: `client/src/gpu_pass_profiler.rs`
- Modify: `client/src/main.rs`

**Step 1: Add initialization system and plugin registration**

Add to `gpu_pass_profiler.rs`:

```rust
use bevy::render::{
    render_graph::RenderGraphApp,
    RenderApp, Render, RenderSet,
};
use bevy::core_pipeline::core_3d::graph::Core3d;
use bevy::core_pipeline::core_3d::graph::Node3d;
use bevy::pbr::graph::NodePbr;

pub fn plugin(app: &mut App) {
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };

    // Add timestamp nodes to the Core3d render graph.
    // Each pair brackets a pass: before writes slot N*2, after writes slot N*2+1.
    render_app
        // Shadow: slots 0,1
        .add_render_graph_node::<TimestampNode>(Core3d, TimestampLabel::BeforeShadow)
        .add_render_graph_node::<TimestampNode>(Core3d, TimestampLabel::AfterShadow)
        // Prepass: slots 2,3
        .add_render_graph_node::<TimestampNode>(Core3d, TimestampLabel::BeforePrepass)
        .add_render_graph_node::<TimestampNode>(Core3d, TimestampLabel::AfterPrepass)
        // Main pass: slots 4,5
        .add_render_graph_node::<TimestampNode>(Core3d, TimestampLabel::BeforeMainPass)
        .add_render_graph_node::<TimestampNode>(Core3d, TimestampLabel::AfterMainPass)
        // Post-processing: slots 6,7
        .add_render_graph_node::<TimestampNode>(Core3d, TimestampLabel::BeforePostProcess)
        .add_render_graph_node::<TimestampNode>(Core3d, TimestampLabel::AfterPostProcess)
        // Resolve node
        .add_render_graph_node::<TimestampResolveNode>(Core3d, TimestampResolveLabel);

    // Wire edges: each "before" node runs before its pass, each "after" runs after.
    render_app
        // Shadow span
        .add_render_graph_edge(Core3d, TimestampLabel::BeforeShadow, NodePbr::EarlyShadowPass)
        .add_render_graph_edge(Core3d, NodePbr::LateShadowPass, TimestampLabel::AfterShadow)
        .add_render_graph_edge(Core3d, TimestampLabel::AfterShadow, Node3d::StartMainPass)
        // Prepass span
        .add_render_graph_edge(Core3d, TimestampLabel::BeforePrepass, Node3d::EarlyPrepass)
        .add_render_graph_edge(Core3d, Node3d::EndPrepasses, TimestampLabel::AfterPrepass)
        .add_render_graph_edge(Core3d, TimestampLabel::AfterPrepass, Node3d::StartMainPass)
        // Main pass span
        .add_render_graph_edge(Core3d, TimestampLabel::BeforeMainPass, Node3d::StartMainPass)
        .add_render_graph_edge(Core3d, Node3d::EndMainPass, TimestampLabel::AfterMainPass)
        .add_render_graph_edge(Core3d, TimestampLabel::AfterMainPass, Node3d::StartMainPassPostProcessing)
        // Post-processing span
        .add_render_graph_edge(Core3d, TimestampLabel::BeforePostProcess, Node3d::StartMainPassPostProcessing)
        .add_render_graph_edge(Core3d, Node3d::EndMainPassPostProcessing, TimestampLabel::AfterPostProcess)
        // Resolve runs after everything
        .add_render_graph_edge(Core3d, TimestampLabel::AfterPostProcess, TimestampResolveLabel);
}
```

Note: `TimestampNode` needs to implement `FromWorld` for `add_render_graph_node`. Since each node needs a different slot index, we'll need to handle this. The simplest approach: make `TimestampNode` store its slot based on its label, looked up from world. Alternative: use a different approach — register nodes manually via direct graph access.

Actually, `add_render_graph_node` requires `Node + FromWorld`. Since each `TimestampNode` needs a unique slot, we should instead access the render graph directly:

```rust
// In plugin(), after getting render_app:
{
    let render_world = render_app.world_mut();
    let mut graph = render_world.resource_mut::<bevy::render::render_graph::RenderGraph>();
    let core3d = graph.get_sub_graph_mut(Core3d).expect("Core3d sub-graph must exist");

    // Add timestamp nodes with their slot indices
    core3d.add_node(TimestampLabel::BeforeShadow, TimestampNode::new(0));
    core3d.add_node(TimestampLabel::AfterShadow, TimestampNode::new(1));
    core3d.add_node(TimestampLabel::BeforePrepass, TimestampNode::new(2));
    core3d.add_node(TimestampLabel::AfterPrepass, TimestampNode::new(3));
    core3d.add_node(TimestampLabel::BeforeMainPass, TimestampNode::new(4));
    core3d.add_node(TimestampLabel::AfterMainPass, TimestampNode::new(5));
    core3d.add_node(TimestampLabel::BeforePostProcess, TimestampNode::new(6));
    core3d.add_node(TimestampLabel::AfterPostProcess, TimestampNode::new(7));
    core3d.add_node(TimestampResolveLabel, TimestampResolveNode);

    // Wire edges
    core3d.add_node_edge(TimestampLabel::BeforeShadow, NodePbr::EarlyShadowPass);
    core3d.add_node_edge(NodePbr::LateShadowPass, TimestampLabel::AfterShadow);
    core3d.add_node_edge(TimestampLabel::AfterShadow, Node3d::StartMainPass);

    core3d.add_node_edge(TimestampLabel::BeforePrepass, Node3d::EarlyPrepass);
    core3d.add_node_edge(Node3d::EndPrepasses, TimestampLabel::AfterPrepass);
    core3d.add_node_edge(TimestampLabel::AfterPrepass, Node3d::StartMainPass);

    core3d.add_node_edge(TimestampLabel::BeforeMainPass, Node3d::StartMainPass);
    core3d.add_node_edge(Node3d::EndMainPass, TimestampLabel::AfterMainPass);
    core3d.add_node_edge(TimestampLabel::AfterMainPass, Node3d::StartMainPassPostProcessing);

    core3d.add_node_edge(TimestampLabel::BeforePostProcess, Node3d::StartMainPassPostProcessing);
    core3d.add_node_edge(Node3d::EndMainPassPostProcessing, TimestampLabel::AfterPostProcess);

    core3d.add_node_edge(TimestampLabel::AfterPostProcess, TimestampResolveLabel);
}
```

**Step 2: Add initialization system for GPU resources**

Add a system that runs once in `RenderStartup` to create the QuerySet and buffers:

```rust
pub fn init_gpu_timestamps(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
) {
    let device = render_device.wgpu_device();

    // Check if timestamp queries are supported
    if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
        warn!("GPU timestamp queries not supported — F10 profiler disabled");
        return;
    }

    let query_set = device.create_query_set(&QuerySetDescriptor {
        label: Some("gpu_pass_profiler_queries"),
        count: NUM_TIMESTAMP_SLOTS,
        ty: QueryType::Timestamp,
    });

    let resolve_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("gpu_pass_profiler_resolve"),
        size: RESOLVE_BUFFER_SIZE,
        usage: BufferUsages::QUERY_RESOLVE | BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let readback_buffers = std::array::from_fn(|i| {
        device.create_buffer(&BufferDescriptor {
            label: Some(&format!("gpu_pass_profiler_readback_{i}")),
            size: RESOLVE_BUFFER_SIZE,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        })
    });

    let timestamp_period = render_queue.get_timestamp_period();

    commands.insert_resource(GpuTimestampState {
        query_set,
        resolve_buffer,
        readback_buffers,
        frame_index: 0,
        active: false,
        timestamp_period,
    });
}
```

Register it in the plugin:

```rust
render_app.add_systems(bevy::render::RenderStartup, init_gpu_timestamps);
```

**Step 3: Replace gpu_profiler in main.rs**

In `main.rs`, change `gpu_profiler::plugin` to `gpu_pass_profiler::plugin`. Add `mod gpu_pass_profiler;`.

**Step 4: Verify compilation**

Run: `cargo check`
Expected: compiles clean

**Step 5: Commit**

```
feat: register GPU timestamp nodes in render graph
```

---

### Task 4: Add async readback and main-world profiler state

**Files:**
- Modify: `client/src/gpu_pass_profiler.rs`

**Step 1: Add main-world profiler resource and extract mechanism**

```rust
use bevy::render::extract_resource::ExtractResource;
use std::time::Duration;

const RECORD_DURATION: Duration = Duration::from_secs(10);

/// Per-span timing samples collected over the recording period.
#[derive(Default, Clone)]
pub struct PassTimingSamples {
    pub name: &'static str,
    pub samples_ms: Vec<f32>,
}

/// Main-world resource: controls profiling state and accumulates results.
#[derive(Resource, Clone)]
pub struct GpuPassProfilerControl {
    pub active: bool,
}

impl ExtractResource for GpuPassProfilerControl {
    type Source = GpuPassProfilerControl;
    fn extract_resource(source: &Self::Source) -> Self {
        source.clone()
    }
}
```

**Step 2: Add readback system in render world**

This system runs each frame in the render world. It maps the readback buffer from 2 frames ago, reads the timestamps, and stores results in a render-world resource that gets extracted to the main world:

```rust
/// Results from one frame's timestamp queries.
#[derive(Resource, Default, Clone)]
pub struct GpuTimestampResults {
    /// Per-span durations in milliseconds. One entry per SPAN_NAMES element.
    pub span_ms: Vec<f32>,
    /// Whether valid data was read this frame.
    pub valid: bool,
}

impl ExtractResource for GpuTimestampResults {
    type Source = GpuTimestampResults;
    fn extract_resource(source: &Self::Source) -> Self {
        source.clone()
    }
}

pub fn readback_timestamps(
    mut state: ResMut<GpuTimestampState>,
    mut results: ResMut<GpuTimestampResults>,
) {
    results.valid = false;

    if !state.active || state.frame_index < 2 {
        state.frame_index += 1;
        return;
    }

    // Map the readback buffer from 2 frames ago
    let readback_idx = ((state.frame_index - 2) % 2) as usize;
    let buffer = &state.readback_buffers[readback_idx];
    let slice = buffer.slice(..);

    // Non-blocking: check if already mapped (it should be after 2 frames)
    slice.map_async(MapMode::Read, |_| {});
    state.readback_buffers[readback_idx]
        .slice(..)
        .map_async(MapMode::Read, |_| {});

    // Poll to complete the mapping
    // Note: this is a blocking poll but the buffer should already be ready
    // after 2 frames of latency
    let device = ... // We need device access here

    // Actually, the cleanest pattern: map at end of frame, read at start of next.
    // We'll use a callback-based approach or just poll once.

    state.frame_index += 1;
}
```

Actually, the readback pattern needs refinement. The standard approach in wgpu:

1. After `resolve + copy_buffer_to_buffer` in the resolve node, the readback buffer has pending data
2. Next frame: call `buffer.slice(..).map_async(MapMode::Read, callback)`
3. Call `device.poll(Maintain::Wait)` to ensure the map completes
4. Read the data from `buffer.slice(..).get_mapped_range()`
5. Call `buffer.unmap()`

Since we're in the render world and have access to `RenderDevice`, we can poll. The 2-frame latency ensures the GPU has finished writing. Here's the corrected readback system:

```rust
pub fn readback_timestamps(
    mut state: ResMut<GpuTimestampState>,
    mut results: ResMut<GpuTimestampResults>,
    render_device: Res<RenderDevice>,
) {
    results.valid = false;

    if !state.active {
        state.frame_index += 1;
        return;
    }

    if state.frame_index >= 2 {
        let readback_idx = ((state.frame_index - 2) % 2) as usize;
        let buffer = &state.readback_buffers[readback_idx];
        let slice = buffer.slice(..);

        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        render_device.wgpu_device().poll(wgpu::Maintain::Wait);

        if rx.recv().map(|r| r.is_ok()).unwrap_or(false) {
            let data = slice.get_mapped_range();
            let timestamps: &[u64] =
                bytemuck::cast_slice(&data);

            let period_ms = state.timestamp_period as f64 / 1_000_000.0;
            let mut span_ms = Vec::with_capacity(SPAN_NAMES.len());

            for i in 0..SPAN_NAMES.len() {
                let start = timestamps[i * 2];
                let end = timestamps[i * 2 + 1];
                if end >= start {
                    span_ms.push(((end - start) as f64 * period_ms) as f32);
                } else {
                    span_ms.push(0.0); // Overflow/wrap
                }
            }

            drop(data);
            buffer.unmap();

            results.span_ms = span_ms;
            results.valid = true;
        }
    }

    state.frame_index += 1;
}
```

Register this system in `Render` schedule, early (before `RenderSet::Prepare`).

**Step 3: Add extract system to sync control from main → render world**

```rust
pub fn sync_profiler_control(
    control: Option<Res<GpuPassProfilerControl>>,
    mut state: ResMut<GpuTimestampState>,
) {
    if let Some(control) = control {
        let was_active = state.active;
        state.active = control.active;
        if control.active && !was_active {
            state.frame_index = 0; // Reset on activation
        }
    }
}
```

**Step 4: Verify compilation**

Run: `cargo check`
Expected: compiles clean

Note: We need `bytemuck` as a dependency. Check if it's already available via bevy re-export, otherwise add it to `client/Cargo.toml`. Bevy re-exports bytemuck, so `bevy::render::render_resource::bytemuck` or similar should work. If not, add `bytemuck = "1"` to dependencies.

**Step 5: Commit**

```
feat: add GPU timestamp readback and main-world profiler state
```

---

### Task 5: Add F10 toggle, recording, and terminal report

**Files:**
- Modify: `client/src/gpu_pass_profiler.rs`

**Step 1: Add the F10 toggle system and recording state**

```rust
use bevy::input::common_conditions::input_just_pressed;

#[derive(Resource)]
struct GpuPassRecording {
    elapsed: Duration,
    span_samples: Vec<Vec<f32>>, // One Vec<f32> per span
    cpu_frame_times: Vec<f32>,
}

#[derive(Component)]
struct GpuPassProfilerOverlay;

fn toggle_gpu_pass_profiler(
    mut commands: Commands,
    existing: Option<Res<GpuPassRecording>>,
    control: Option<Res<GpuPassProfilerControl>>,
    overlay: Query<Entity, With<GpuPassProfilerOverlay>>,
) {
    if existing.is_some() {
        // Cancel
        commands.remove_resource::<GpuPassRecording>();
        commands.insert_resource(GpuPassProfilerControl { active: false });
        for entity in &overlay {
            commands.entity(entity).despawn();
        }
        info!("GPU pass profiler cancelled.");
        return;
    }

    commands.insert_resource(GpuPassProfilerControl { active: true });
    commands.insert_resource(GpuPassRecording {
        elapsed: Duration::ZERO,
        span_samples: vec![Vec::with_capacity(1024); SPAN_NAMES.len()],
        cpu_frame_times: Vec::with_capacity(1024),
    });

    // Spawn overlay
    commands.spawn((
        GpuPassProfilerOverlay,
        Text::new(format!("GPU PASS PROFILE  {}s", RECORD_DURATION.as_secs())),
        TextFont { font_size: 18.0, ..default() },
        TextColor(crate::ui::colors::ACID_GREEN),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(16.0),
            right: Val::Px(16.0),
            ..default()
        },
    ));

    info!(
        "GPU pass profiler started — recording for {}s (F10 to cancel)...",
        RECORD_DURATION.as_secs()
    );
}
```

**Step 2: Add the tick system that accumulates samples**

```rust
fn tick_gpu_pass_profiler(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut recording: ResMut<GpuPassRecording>,
    results: Option<Res<GpuTimestampResults>>,
    mut overlay: Query<&mut Text, With<GpuPassProfilerOverlay>>,
    overlay_entities: Query<Entity, With<GpuPassProfilerOverlay>>,
) {
    let delta = time.delta();
    recording.elapsed += delta;
    recording.cpu_frame_times.push(delta.as_secs_f32() * 1000.0);

    // Accumulate GPU timestamp results if available
    if let Some(results) = results {
        if results.valid && results.span_ms.len() == SPAN_NAMES.len() {
            for (i, &ms) in results.span_ms.iter().enumerate() {
                recording.span_samples[i].push(ms);
            }
        }
    }

    // Update overlay
    let remaining = RECORD_DURATION.saturating_sub(recording.elapsed);
    for mut text in &mut overlay {
        text.0 = format!("GPU PASS PROFILE  {:.0}s", remaining.as_secs_f32().ceil());
    }

    // Done?
    if recording.elapsed >= RECORD_DURATION {
        let recording = commands.remove_resource::<GpuPassRecording>().unwrap();
        commands.insert_resource(GpuPassProfilerControl { active: false });
        for entity in &overlay_entities {
            commands.entity(entity).despawn();
        }
        log_gpu_pass_report(&recording);
    }
}
```

**Step 3: Add the report formatter**

```rust
fn log_gpu_pass_report(recording: &GpuPassRecording) {
    let frame_count = recording.cpu_frame_times.len();
    let gpu_sample_count = recording.span_samples[0].len();

    let mut report = format!(
        "\n=== GPU PASS TIMING ({}s, {} frames, {} GPU samples) ===\n",
        RECORD_DURATION.as_secs(),
        frame_count,
        gpu_sample_count,
    );
    report.push_str(&format!(
        "{:<28} {:>8} {:>8} {:>8} {:>10}\n",
        "Pass", "Avg ms", "p50", "p95", "% of frame"
    ));

    let mut total_avg = 0.0f32;
    let mut total_p50 = 0.0f32;
    let mut total_p95 = 0.0f32;

    for (i, &name) in SPAN_NAMES.iter().enumerate() {
        let samples = &recording.span_samples[i];
        if samples.is_empty() {
            continue;
        }

        let (avg, p50, p95) = compute_stats(samples);
        total_avg += avg;
        total_p50 += p50;
        total_p95 += p95;

        let cpu_avg = compute_stats(&recording.cpu_frame_times).0;
        let pct = if cpu_avg > 0.0 { avg / cpu_avg * 100.0 } else { 0.0 };

        report.push_str(&format!(
            "{:<28} {:>8.1} {:>8.1} {:>8.1} {:>9.1}%\n",
            name, avg, p50, p95, pct,
        ));
    }

    let cpu_stats = compute_stats(&recording.cpu_frame_times);
    report.push_str(&format!(
        "{:<28} {:>8.1} {:>8.1} {:>8.1}\n",
        "Total GPU measured", total_avg, total_p50, total_p95,
    ));
    report.push_str(&format!(
        "{:<28} {:>8.1} {:>8.1} {:>8.1}\n",
        "Frame (CPU)", cpu_stats.0, cpu_stats.1, cpu_stats.2,
    ));

    info!("{report}");
}

fn compute_stats(samples: &[f32]) -> (f32, f32, f32) {
    if samples.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let count = sorted.len();
    let avg = sorted.iter().sum::<f32>() / count as f32;
    let p50 = sorted[count * 50 / 100];
    let p95 = sorted[(count * 95 / 100).min(count - 1)];
    (avg, p50, p95)
}
```

**Step 4: Wire all systems in the plugin function**

Update `plugin()` to register main-world systems:

```rust
pub fn plugin(app: &mut App) {
    // Main world systems
    app.add_systems(
        Update,
        toggle_gpu_pass_profiler.run_if(input_just_pressed(KeyCode::F10)),
    );
    app.add_systems(
        Update,
        tick_gpu_pass_profiler.run_if(resource_exists::<GpuPassRecording>),
    );

    // Render world setup (existing code from Task 3)
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return; };

    // ... render graph node registration from Task 3 ...

    // Register render-world systems
    render_app
        .add_systems(bevy::render::RenderStartup, init_gpu_timestamps)
        .add_systems(
            Render,
            (sync_profiler_control, readback_timestamps)
                .chain()
                .in_set(RenderSet::Prepare),
        )
        .init_resource::<GpuTimestampResults>();

    // Extract: main → render (control), render → main (results)
    // For control: use ExtractResourcePlugin
    // For results: need render → main, which ExtractResource doesn't do.
    // Instead, we'll use a custom extraction or just read from render world.
}
```

**Important note on data flow direction:** `ExtractResource` goes main → render. For GPU results going render → main, we need a different approach. Options:

1. Use `extract_component` or a custom system that runs in `ExtractSchedule` to copy data the other direction
2. Store accumulated samples in the main world and only extract per-frame results via a shared channel

The simplest: use a `crossbeam` or `std::sync::mpsc` channel. The render-world readback system sends per-frame results through the channel, and the main-world tick system receives them.

Actually even simpler: just use an `Arc<Mutex<Vec<f32>>>` shared between both worlds. Or store the GpuTimestampResults in the main world and have the render world write to it directly (both worlds share the same memory in Bevy's architecture — but ExtractSchedule copies resources, not shares them).

The cleanest Bevy-native approach: have the readback system in the render world push results into a resource, then extract it to the main world in `ExtractSchedule`. But `ExtractResource` copies main→render, not the reverse.

**Revised approach:** Use `Res<RenderDevice>` in a main-world system? No, render resources aren't available in the main world.

**Best approach:** Use a `std::sync::mpsc::Receiver<Vec<f32>>` in the main world and a `Sender` in the render world. Set them up during plugin init.

```rust
use std::sync::mpsc;

#[derive(Resource)]
struct GpuTimestampReceiver(mpsc::Receiver<Vec<f32>>);

#[derive(Resource)]
struct GpuTimestampSender(mpsc::Sender<Vec<f32>>);
```

Create the channel in `plugin()`, insert `Receiver` in main world, `Sender` in render world. The render-world readback system sends results through the channel. The main-world tick system tries `recv()` each frame.

**Step 5: Verify compilation**

Run: `cargo check`
Expected: compiles clean

**Step 6: Commit**

```
feat: add F10 toggle, recording, and terminal report for GPU pass profiler
```

---

### Task 6: Delete old gpu_profiler.rs and verify end-to-end

**Files:**
- Delete: `client/src/gpu_profiler.rs`
- Modify: `client/src/main.rs` (remove old module, ensure new one is registered)

**Step 1: Remove old module and verify compilation**

Delete `gpu_profiler.rs`, remove `mod gpu_profiler;` from main.rs.

Run: `cargo check`
Expected: compiles clean

**Step 2: Manual test**

Run: `cargo run --release`

1. Load into gameplay with enemies
2. Press F10
3. See "GPU PASS PROFILE 10s" overlay
4. Wait 10 seconds
5. Check terminal for per-pass timing report
6. Press F10 again while recording to verify cancel works

**Step 3: Commit**

```
feat: replace phase-based GPU profiler with per-pass timestamp profiler
```
