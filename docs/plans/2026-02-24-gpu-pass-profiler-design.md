# GPU Pass Profiler Design

## Problem

The current F10 GPU profiler uses CPU frame time deltas between scene manipulation phases (hide enemies, disable shadows, etc.) to infer GPU costs. This is indirect and imprecise — it can't tell you which render pass is expensive, only which scene change reduces total frame time. We need per-render-pass GPU timing to diagnose bottlenecks before attempting optimizations.

## Solution

Replace F10 with a GPU timestamp query profiler that measures actual GPU-side nanosecond timing per render pass. Press F10, record for 10 seconds, print a per-pass timing report to the terminal.

## Architecture

### Timestamp Wrapper Nodes

Insert lightweight render graph nodes before and after each major pass. Each node calls `encoder.write_timestamp()` on a shared `QuerySet`. No render passes are created — just timestamp writes between existing passes.

### Passes Measured (5 spans, 8 unique timestamp slots)

| Span | Before | After |
|------|--------|-------|
| Shadow | before `EarlyShadowPass` | before `StartMainPass` |
| Prepass | before `EarlyPrepass` | after `EndPrepasses` |
| Main pass | before `StartMainPass` | after `EndMainPass` |
| Post-processing | before `StartMainPassPostProcessing` | after `EndMainPassPostProcessing` |
| Full frame | reuses prepass-start | reuses post-end |

### Async Readback (2-frame latency)

```
Frame N:   write timestamps → resolve to gpu_buffer → copy to readback_buffer[N%2]
Frame N+1: map readback_buffer[(N-1)%2] → read results → store in stats
```

Ping-pong between two readback buffers to avoid GPU stalls.

### Resources

- `GpuTimestampState` (render world): QuerySet, resolve buffer, two readback buffers, frame counter
- `GpuPassProfilerState` (main world): per-pass timing samples, F10 toggle, recording state machine
- Results flow from render world to main world via `ExtractResource`

### Feature Requirements

- `TIMESTAMP_QUERY` wgpu feature requested at device creation via `WgpuSettings`
- Graceful skip if feature unsupported (log warning on F10 press)
- Native only — no WASM/WebGPU support needed

## Output Format

```
=== GPU PASS TIMING (10s, 602 frames) ===
Pass                        Avg ms      p50      p95    % of frame
early_shadow_pass             3.2      3.1      3.8       5.1%
late_shadow_pass              2.1      2.0      2.5       3.3%
prepass                      22.1     21.8     24.3      35.2%
main_pass                    18.4     18.1     20.2      29.3%
post_processing               1.8      1.7      2.0       2.9%
Total measured               47.6     46.7     52.8
Frame (CPU)                  62.7     62.0     67.4
```

## Keybind

F10 (replaces the existing phase-based profiler).
