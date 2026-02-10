# Animated Zombie Enemies — Status & Next Steps

## What Was Done

### Animated GLTF zombie scenes replace capsules
- `SharedEnemyAssets` stores `Handle<Scene>`, shared red `Handle<StandardMaterial>`, pre-built `AnimationGraph` with 3 zombie clips (ZombieIdle, ZombieWalkForward, ZombieScratch), and clip node indices
- Setup runs on `OnEnter(Screen::Gameplay)` (not `Startup`) so the GLTF is loaded
- `on_enemy_added` spawns a child entity with `SceneRoot` at y=-1.0, with `observe(prepare_enemy_scene)`
- `prepare_enemy_scene` (on `SceneInstanceReady`) finds `AnimationPlayer` descendant, attaches shared animation graph, starts ZombieIdle looping, replaces all materials with shared red material, stores `EnemyAnimations` on parent
- `animate_enemies` system (in `PlayAnimations` set) watches `Changed<EnemyBehavior>` and transitions with 200ms blend

### Distance-based animation culling
- `cull_distant_enemy_animations` pauses `AnimationPlayer` (speed=0) for enemies >30m from camera

### Spatial hash for O(n) separation
- Enemy-enemy separation uses `HashMap<(i32, i32), Vec<(Entity, Vec3)>>` grid
- Cell size = `ENEMY_SEPARATION_RADIUS`, checks 9 neighboring cells per enemy

## Current Performance

~40 FPS with 1000 enemies on M1 Pro. Not acceptable — needs optimization.

## Profiling TODO

Before optimizing, profile to identify the actual bottleneck:
- **CPU animation evaluation** — each enemy has its own `AnimationPlayer` evaluating bone matrices every frame
- **Draw calls** — even with shared scene handle, skinned meshes may not batch
- **Transform propagation** — 1000 entity hierarchies (enemy -> scene -> armature -> bones) is a lot of transform propagation
- **AI system** — spatial hash should be fast, but worth confirming

Use `tracy` or Bevy's built-in trace spans to identify which system is eating the budget.

## Optimization Options (ordered by likely impact)

### A. Reduce bone count / simpler mesh
- Current player.glb has full skeleton. Enemies don't need finger bones, facial bones, etc.
- Export a separate `zombie.glb` with a reduced skeleton (10-15 bones vs 50+)
- Huge impact on animation evaluation AND transform propagation

### B. VisibilityRange (LOD)
- Use Bevy's `VisibilityRange` component to swap animated mesh for a simple capsule/billboard beyond ~15m
- Only nearby enemies pay the full animation cost
- `VisibilityRange { start: 0.0..2.0, end: 15.0..20.0 }` with capsule fallback

### C. GPU skinning / baked animation
- Pre-bake bone matrices into SSBO at build time
- Custom vertex shader samples from a texture/buffer — zero CPU animation cost
- Most complex option, defer until simpler options are exhausted

### D. Animation update frequency reduction
- Only evaluate animations every 2nd or 3rd frame for distant enemies
- Cheaper than full culling, smoother than hard cutoff

### E. Shared AnimationGraph instance
- Currently each enemy gets `AnimationGraphHandle(shared.animation_graph.clone())` — the graph handle is shared but each `AnimationPlayer` evaluates independently
- Investigate if Bevy supports instanced animation evaluation

## Files Modified

| File | What changed |
|------|-------------|
| `client/src/combat/enemy.rs` | Replaced capsule with GLTF scene, animation system, culling, spatial hash |
| `client/src/combat/components.rs` | `EnemyAnimations` was already defined (unchanged) |
