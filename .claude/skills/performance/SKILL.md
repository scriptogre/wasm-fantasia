---
name: performance
description: Scan the codebase for performance issues using data-oriented design principles. Use when the user wants to find optimization opportunities, reduce allocations, or improve hot-path performance.
argument-hint: "[file or directory to focus on]"
---

# Performance Audit

Scan the codebase (or the specific file/directory in `$ARGUMENTS` if provided) for performance issues using data-oriented design principles. These rules apply universally but are **especially critical in WASM targets** where heap allocation is 10-20x more expensive than native due to dlmalloc in linear memory.

## Audit Checklist

Work through each rule below. For every finding, report: the file and line, the current code, what's wrong, the fix, and a severity (P0/P1/P2). Skip rules that have no findings.

### 1. Only use strings when a human is reading them

Look for `String` fields on structs that represent a **fixed set of values** (enums encoded as strings). Especially in:
- Database schema types and table row structs
- Network protocol messages
- Hot-path data structures iterated in bulk

Red flags: `.to_string()` on string literals, `"some_constant".into()`, `String` fields that only ever hold 2-5 known values, `Stat::Custom("...".into())` patterns.

Fix: Replace with `u8` constants, enum variants, or `&'static str` where ownership isn't needed.

### 2. Store related data near each other

Look for structs where position/velocity/transform fields are interleaved with unrelated data (strings, flags, metadata). This hurts cache locality when iterating over many entities to read spatial data.

Fix: Group fields by access pattern — spatial data together, metadata together, strings at the end.

### 3. Do tasks in bulk

Look for patterns that process items one at a time when batch processing is available:
- Individual DB inserts in a loop instead of batch insert
- Per-element network sends instead of batched messages
- Repeated small allocations instead of pre-allocated buffers

### 4. Do tasks ahead of time / Cache unchanging data

Look for **deterministic computations repeated on every call**:
- Building rule trees, config objects, or lookup tables inside hot functions
- Trig functions (`cos`, `sin`, `to_radians`) on values that rarely change
- Regex compilation inside loops

Red flags: `let rules = build_rules()` inside a reducer/system called every tick, `let re = Regex::new(...)` in a loop.

Fix: `thread_local!`, `LazyLock`, `OnceCell`, or precompute and store on the entity.

### 5. Do tasks in parallel

Look for independent iterations over large collections that could use `par_iter()` (rayon) on native, or be split into independent systems in Bevy.

Note: SpacetimeDB WASM modules are single-threaded — parallelism only applies to the client.

### 6. Make enums only as large as they need to be

Look for enums where one variant contains a large type (e.g., `String`, `Vec`, `Box<dyn ...>`) that inflates the size of all variants. Every instance of the enum pays the cost of the largest variant.

Red flags: `Custom(String)` or `Other(Vec<u8>)` variants on enums where 95%+ of usage is the small variants.

Fix: `Box` the large variant (`Custom(Box<String>)`), or promote frequent "custom" values to first-class variants.

### 7. Avoid unnecessary allocations in hot paths

Look for:
- `.collect::<Vec<_>>()` followed by iteration (collect-then-iterate) — iterate directly or collect only the fields you need (e.g., IDs instead of full structs)
- `HashMap::entry(key.clone())` in loops — use `get_mut`/`insert` to clone only on first insertion
- `format!()` or `String::from()` in per-entity loops
- Cloning structs with String fields just to pass to an API that could take references

### 8. Use state arrays instead of booleans

Look for `bool` fields used to filter entities (e.g., `is_active`, `online`). An alternative is separate collections where membership implies state, eliminating filter passes.

Note: In Bevy ECS, this is idiomatic via marker components. In SpacetimeDB, separate tables may not be practical due to lack of atomic cross-table moves. Flag findings but assess practicality.

### 9. Cut operations you don't need

Look for:
- Sorting when order doesn't matter
- Cloning data that's about to be dropped
- Defensive copies when the original won't be mutated
- Recomputing values available from a previous step

### 10. Avoid exponential/quadratic complexity

Look for:
- Nested loops over the same collection — O(N^2)
- `.find()` or `.contains()` inside a loop — build a HashMap/HashSet first
- Broad-phase collision using all-pairs instead of spatial partitioning

### 11. Minimize allocation in WASM specifically

If the project targets WASM (check for `wasm32` targets, `[lib] crate-type = ["cdylib"]`, or SpacetimeDB modules):
- Every `String` clone/allocation is ~10-20x more expensive than native
- Prefer `u8`/`u16`/`u32` over `String` for encoded values
- Prefer `&str` borrows over `String` ownership where lifetime allows
- Pre-size `Vec::with_capacity()` when the count is known

## GPU / Rendering Checklist

Work through each rule below in the same way as the CPU checklist. These apply to the Bevy client and are especially important for WebGPU where driver overhead is higher than native Vulkan/Metal.

### 12. Share material and mesh handles — never allocate per entity

Every unique `Handle<Mesh>` + `Handle<Material>` pair is a separate draw call. Thousands of enemies should use the same handles.

Red flags: `materials.add()` or `meshes.add()` inside a spawn loop or an update system. `asset_server.load()` called repeatedly for the same path inside a loop.

Fix: Create the handle once in a setup system, store it in a `Resource`, clone the handle (not the asset) when spawning.

### 13. Never create or swap material/mesh handles per frame

Changing a `Handle<StandardMaterial>` on an entity forces a bind group rebind. Modifying material properties (color, emissive) via `Assets<StandardMaterial>.get_mut()` also invalidates the bind group.

Red flags: `*handle = materials.add(...)` inside a system that runs every frame. `materials.get_mut(handle).base_color = ...` on many entities per frame.

Fix: Drive per-entity visual variation through components and custom shader uniforms (SSBO), not material properties. Use a shared material with instance data.

### 14. Minimize transparent / alpha-blended entities

Transparent objects can't use early depth rejection, must be sorted back-to-front, and cause overdraw. Each overlapping transparent layer re-shades the same pixel.

Red flags: `AlphaMode::Blend` on materials used by many entities. Layered transparent sprites/particles covering large screen areas. Particle systems with high spawn counts and long lifetimes creating dense overlap.

Fix: Use `AlphaMode::Opaque` wherever possible. Prefer additive blending (`AlphaMode::Add`) for VFX — it's order-independent and cheaper. Cap particle counts. Use short lifetimes so particles don't accumulate.

### 15. Use `NoFrustumCulling` sparingly

Bevy frustum-culls by default. Adding `NoFrustumCulling` means the entity is always submitted to the GPU regardless of camera visibility.

Red flags: `NoFrustumCulling` on anything other than skyboxes or full-screen post-process quads. Bulk-spawned entities with `NoFrustumCulling`.

### 16. Implement distance-based LOD or visibility cutoff for large entity counts

At 10K+ entities, even frustum-culled meshes that are tiny on screen waste vertex processing.

Red flags: No `VisibilityRange` or manual LOD system when entity counts exceed ~1000. All entities use the same high-poly mesh regardless of camera distance.

Fix: Use Bevy's `VisibilityRange` component to hide distant entities entirely. Or swap to a lower-poly mesh beyond a distance threshold. Consider enabling the `meshlet` feature for GPU-driven culling when available.

### 17. Avoid Vec3 in GPU buffer structs — use Vec4 for alignment

GPU uniform/storage buffers follow std140/std430 alignment. `Vec3` (12 bytes) wastes 4 bytes of padding to reach 16-byte alignment. Using `Vec4` costs the same memory but avoids hidden padding and potential alignment bugs.

Red flags: `Vec3` fields in `#[derive(ShaderType)]` structs. Multiple `Vec3` fields in a row in uniform buffers.

Fix: Replace `Vec3` with `Vec4` (set w to 0.0 or 1.0). Or add an explicit `_padding: f32` field after each `Vec3`.

### 18. Keep shadow caster count low

Every shadow-casting entity is re-rendered per shadow cascade. With 2 cascades and 10K entities, that's 20K extra draw submissions.

Red flags: Large numbers of entities with shadows enabled (the default). Small or distant entities casting shadows nobody can see.

Fix: Add `NotShadowCaster` to entities that don't need shadows (enemies, particles, debris, small props). Keep shadow cascade count and distance low.

### 19. Minimize unique render pipelines

Each unique combination of (shader, material type, mesh vertex layout, render features) compiles a separate GPU pipeline. Pipeline switches are expensive.

Red flags: Many custom `Material` trait implementations. Materials with feature flags that create a combinatorial explosion of shader variants.

Fix: Keep custom material types to a minimum (<5). Use uniform data for variation, not separate material types. Consolidate similar materials.

### 20. Don't load assets in update systems

`asset_server.load()` in a per-frame system triggers repeated filesystem/cache lookups even if the asset is already loaded.

Red flags: `asset_server.load("...")` inside any system that isn't gated by `run_once`, `OnEnter`, or a startup schedule.

Fix: Load all assets during setup or state transitions. Store handles in Resources.

### 21. Batch entity spawning with `Commands::spawn_batch`

Spawning entities one at a time with `commands.spawn()` in a loop has per-call overhead. `spawn_batch` amortizes it.

Red flags: `for ... { commands.spawn(...); }` loops spawning 100+ entities.

Fix: Use `commands.spawn_batch(iterator)` when spawning many identical-structure entities. Or use `commands.spawn_empty()` with `insert_batch()`.

## Output Format

After the audit, produce a summary table:

```
| Priority | Issue | Files | Effort |
|----------|-------|-------|--------|
| P0 | ... | ... | Easy/Medium/Hard |
```

Then ask the user which findings to implement. For any finding that can be benchmarked, suggest adding a criterion benchmark **before** implementing the fix to establish a baseline.
