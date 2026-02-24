# Compute Pre-Skinning for VAT Enemies

## Problem

With 10K VAT-animated enemies, vertex+prepass costs 37.6ms/frame (80% of frame time). Each vertex independently samples the VAT texture for position and normal — 166M+ texture fetches per frame — even though thousands of enemies share the same animation frame due to snap-to-nearest-frame.

## Solution

A compute shader pre-skins vertices once per unique animation frame (~30), writing results to a storage buffer. The vertex shader reads from this buffer instead of the VAT texture. Texture fetches drop by 99.7%; the vertex shader does only cheap buffer reads.

## Constraints

- Built inside bevy_open_vat (not a client-side addition)
- Replaces the current vertex-shader texture fetch path entirely (no fallback)
- WebGPU only (native + WASM). No WebGL2 support needed.
- Only pre-skin active frames (CPU-side deduplication)

## GPU Buffers

| Buffer | Written by | Read by | Size (10K enemies, 30 frames) | Lifetime |
|--------|-----------|---------|-------------------------------|----------|
| Pre-skinned vertices | Compute | Vertex shader | ~8MB | Each frame |
| Frame table | CPU | Compute | ~480 bytes | Each frame |
| Instance lookup | CPU | Vertex shader | ~40KB | Each frame |
| Vertex UVs | CPU (once) | Compute | ~67KB | Static |

### Pre-skinned vertex struct (GPU)

```wgsl
struct PreSkinnedVertex {
    position: vec4<f32>,  // xyz = animated position offset, w = 0
    normal: vec4<f32>,    // xyz = animated normal, w = 0
}
```

32 bytes per entry, vec4 aligned (no hidden padding per Rule 17).

Layout: `pre_skinned[slot * vertex_count + vertex_id]`

### Frame table

```wgsl
frame_table: array<u32>
```

One entry per unique active frame. Maps slot index to absolute VAT frame index.

### Instance lookup

```wgsl
instance_lookup: array<u32>
```

One entry per entity (indexed by MeshTag). Maps entity to frame slot in pre-skinned buffer.

### Vertex UVs

```wgsl
vertex_uvs: array<vec2<f32>>
```

Extracted once from mesh UV_B attribute. Maps vertex_id to VAT texture column.

## CPU System: `prepare_vat_compute`

Replaces `update_instance_data`. Runs in PostUpdate.

1. Get current time from `Time` resource
2. Iterate all `VatAnimationController` entities
3. For each: compute snapped absolute frame index
   ```
   progress = fract(time * rate + offset)
   absolute_frame = clip.start_frame + round(progress * clip.frame_count)
   ```
4. Deduplicate frames into `Local<HashMap<u32, u32>>` (absolute_frame -> slot)
5. Write `instance_lookup[mesh_tag] = slot` per entity
6. Upload frame table + instance lookup buffers

### Change detection

Skip upload when no controllers changed AND frame indices haven't shifted (animations advance at ~24fps, not 60).

## Compute Shader

Dispatched as render graph node BEFORE main pass and prepass.

```
Dispatch: (active_frame_count, ceil(vertex_count / 64), 1)
Workgroup size: (1, 64, 1)
```

Each thread processes one (frame_slot, vertex_id):

1. Read `abs_frame = frame_table[frame_slot]`
2. Read `uv_x = vertex_uvs[vertex_id].x`
3. `textureLoad` position at `(round(uv_x * (tex_width-1)), abs_frame)`
4. `textureLoad` normal at same X, Y + texture_height/2
5. Decode position (min_pos + raw * range) with Blender->Bevy coord conversion
6. Decode normal (raw * 2 - 1) with coord conversion
7. Write `pre_skinned[frame_slot * vertex_count + vertex_id]`

Uses `textureLoad` (integer texel fetch) instead of `textureSampleLevel` — faster, no sampler needed.

## Modified Vertex Shaders

Both forward and prepass shaders simplified to:

```wgsl
let tag = get_tag(vertex.instance_index);
let slot = instance_lookup[tag];
let idx = slot * vertex_count + vertex_id;
let skinned = pre_skinned[idx];

let new_position = vertex.position + skinned.position.xyz;
let new_normal = skinned.normal.xyz;
// world transform + clip projection (unchanged)
```

Zero texture fetches, zero animation math. The prepass can skip reading the normal.

## Performance Fixes (from audit)

### current_clip: String -> u8

`VatAnimationController.current_clip` changes from `String` to `u8` clip ID. Eliminates heap allocation on every behavior change, speeds up HashMap lookup to array index.

`RemapInfo.animations` changes from `HashMap<String, VatAnimationClip>` to `Vec<VatAnimationClip>` indexed by clip ID.

### VatInstanceData removed

The 16-byte-per-entity animation params buffer is replaced by a 4-byte-per-entity slot index. CPU work per entity drops from "string lookup + rate/offset math + 16-byte write" to "frame index computation + 4-byte write".

### Frame dedup uses Local storage

`Local<HashMap<u32, u32>>` with pre-sized capacity (~64) avoids per-frame allocation.

## Render Graph Integration

New `VatComputeNode` inserted before `MainPass` in Bevy's render graph. The node:

1. Creates a compute pipeline from the pre-skin shader
2. Binds: VAT texture, vertex UVs, frame table, VAT params uniform, output buffer
3. Dispatches `(active_frame_count, ceil(vertex_count/64), 1)`

## Material Binding Changes

`OpenVatExtension` bindings change:

| Binding | Before | After |
|---------|--------|-------|
| 100 | VAT texture (vertex visibility) | Pre-skinned buffer (vertex, read-only) |
| 101 | VAT sampler (vertex) | Instance lookup (vertex, read-only) |
| 102 | VAT params uniform (vertex) | Vertex count uniform (vertex) |
| 103 | Instance data SSBO (vertex) | Removed |

VAT texture + params + vertex UVs move to the compute shader's own bind group.

## Expected Performance

- Current: 10K * 8323 * 2 texture fetches * 2 passes = ~330M texture fetches
- After: 30 * 8323 * 2 texture fetches (compute) + 10K * 8323 * 2 buffer reads (vertex)
- Texture fetches: 330M -> 500K (99.85% reduction)
- Buffer reads are 4-7x faster than texture fetches
- Conservative estimate: vertex+prepass drops from 37.6ms to 10-18ms
