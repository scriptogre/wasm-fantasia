# VAT Enemy Animation — Work In Progress

## Status: 3 issues remaining

### 1. VAT animations not playing (BLOCKING)
Enemies render with correct mesh/material but are stuck in bind pose (no animation).

**What works:**
- Assets load correctly: EXR texture (3389x216), GLB with UV1, remap JSON
- VatAnimationController components spawn correctly: speed=1.0, is_playing=true, correct clip names
- 89 controllers detected in debug
- Material created with correct min_pos, max_pos, frame_count, y_resolution

**What's broken:**
- `debug_vat_ssbo` system in PostUpdate produces NO output at all (not even the controller count line)
- This strongly suggests the system fails to construct or never runs
- `start_time` on controllers was 0.00 (never advancing) — we registered `update_anim_controller` in PostUpdate to fix this but still no animation

**Root cause investigation (next steps):**
1. The `debug_vat_ssbo` system queries `Query<&MeshMaterial3d<ExtendedMaterial<StandardMaterial, OpenVatExtension>>>` — if this system doesn't even log the controller count, something prevents it from running entirely
2. Try simplifying debug system to just `warn!("hello")` with no query params to isolate whether it's a system construction issue
3. Check if `update_instance_data` from bevy_open_vat plugin is actually running (it has similar query parameters)
4. Key insight from source: `update_instance_data` has dirty detection that skips if: no Changed<VatAnimationController>, entity count unchanged, no asset events. If `update_anim_controller` isn't triggering Changed<> properly, SSBO never gets written
5. `update_instance_data` also REASSIGNS MeshTag values sequentially — our code in `apply_vat_to_descendants` also assigns MeshTag. This is fine (plugin overwrites ours) but worth knowing
6. Consider: is the shader actually running? `naga=off` in log filter hides shader compilation errors. Try `naga=warn` temporarily

**How update_instance_data works (from source at `~/.cargo/registry/src/index.crates.io-.../bevy_open_vat-0.18.0/src/system.rs`):**
- rate = speed / duration
- offset = -(start_time * rate) + controller.offset
- Writes Vec<VatInstanceData> to SSBO via `buffer.set_data()`
- Iterates `mat_query: Query<&MeshMaterial3d<ExtendedMaterial<...>>>` to find material handles
- Gets buffer handle from `mat.extension.instance`
- If no controllers Changed AND count same AND no asset events → early return (skips entirely)

### 2. Hit flash broken
`on_hit_flash` in `client/src/combat/vfx.rs` queries `MeshMaterial3d<StandardMaterial>` on `event.target` — fails for VAT enemies which now use `ExtendedMaterial<StandardMaterial, OpenVatExtension>`.

**Fix approach:** Add a parallel system/observer for ExtendedMaterial, or make the hit flash target the enemy entity's child mesh via VatMeshLink, modifying the ExtendedMaterial's base color instead.

### 3. Y position slightly off
Currently -0.15 Y offset on scene child. User says enemies hover slightly. Try -0.2 or -0.25.

## File locations
- `client/src/combat/enemy.rs` — VAT spawn, debug systems, animate_enemies
- `client/src/combat/vfx.rs` — hit flash (broken for VAT)
- `client/src/combat/components.rs` — EnemyBehavior enum, Enemy marker
- `client/src/asset_loading/mod.rs` — Models resource with enemy VAT handles
- `client/src/main.rs` — OpenVatPlugin registered
- Assets: `client/assets/models/zombie_vat/` (zombie.glb, zombie_vat.exr, zombie-remap_info.json)

## Cleanup TODO (after animation works)
- Remove `debug_vat_ssbo` system from enemy.rs
- Remove ZombieIdle/ZombieWalkForward/ZombieScratch from Animation enum in player/animation.rs
