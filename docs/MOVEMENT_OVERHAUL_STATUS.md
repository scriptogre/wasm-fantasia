# Movement Overhaul — Progress

## Completed (Phase 2 & 3: Code Changes)

All code changes are implemented and compiling on native.

| File | Changes |
|------|---------|
| `client/assets/config.ron` | speed: 6, sprint_factor: 2.8, step: 0.55 |
| `client/src/player/mod.rs` | Tnua: acceleration 50, air_accel 20, takeoff_gravity 0, peak_prevention 1.0 |
| `client/src/player/control.rs` | SmoothedInput (asymmetric lerp 12/4), speed-dependent turn rate clamping, TAP_THRESHOLD 0.15, forward momentum 10+18*t, `is_sprinting` on Footstep event |
| `client/src/player/animation.rs` | GENERAL_SPEED 0.065, sprint Alter/Maintain *0.8, sprint detection threshold *1.3 |
| `client/src/player/sound.rs` | Passes `is_sprinting` through Footstep |
| `client/src/combat/vfx.rs` | Sprint-aware dust (10 vs 5 particles), beefier landing (0.8+5t scale, 16+16t particles), jump ground-break crack ring at >30% charge, speed lines during charged jump ascent |
| `client/src/camera/juice.rs` | ScreenShake on LandingImpact (quadratic scaling), FootstepBob with spring dynamics, FOV: sprint +20deg smoothstep, landing punch 10deg, charge narrowing 6deg, falling +15deg |

## In Progress (Phase 1: Mixamo Animations)

### Done
- Exported `player_for_mixamo.fbx` to project root

### Next Steps (Manual)
1. Upload `player_for_mixamo.fbx` to https://mixamo.com
2. Let Mixamo auto-rig the skeleton
3. Pick animations:
   - Heavy/powerful sprint (replacing current light Sprint_Loop)
   - Sprint stop/slide (for deceleration)
   - Sprint start/acceleration burst
   - Heavy landing (alternative to current NinjaJumpLand)
4. Download each as FBX, select "Without Skin" (animation data only)
5. Drop the downloaded FBX files in the project

### Next Steps (Claude)
1. Import Mixamo FBX files into Blender alongside the original model
2. Verify bone mapping
3. Merge and export enriched `player.source.glb`
4. Add new `Animation` enum variants in `animation.rs` (build.rs auto-strips unused clips)
5. Re-tune animation speed multipliers for the new clips

## Not Yet Done

- WASM compilation check (`cargo check -p wasm_fantasia --no-default-features --features web --target wasm32-unknown-unknown`)
- Playtesting and tuning the new values
- Clean up `player_for_mixamo.fbx` from project root after animation work is done
