# Implementation

Technical strategy for achieving the design goals.

## Movement: "Parkour Flow"

The hallmark of PROTOTYPE is that the player almost never stops moving. Build transitions between locomotion modes, not discrete states.

### Core Mechanics

| Mechanic | Implementation |
|----------|----------------|
| Dynamic Acceleration | Speed ramp—holding sprint builds velocity over time, not instant max speed |
| Vertical Scaling | Wall collision while sprinting rotates "up" vector to wall's plane |
| Pressure-Sensitive Jump | Short tap = hop; long hold = charged leap that maintains forward momentum |
| Momentum Conservation | Inherit velocity on state transitions (wall-run → jump adds directional impulse, not reset) |
| Air Control | Higher-than-normal lateral forces while airborne (superhuman steering) |

### State Machine

```rust
#[derive(Component)]
enum MovementState {
    Sprinting,
    WallRunning,
    Gliding,
    Dashing,
}
```

### Context Detection (Raycasting)

Use `ShapeCast` to detect obstacles before collision:
- **Low Obstacle** → Vault animation + velocity boost
- **High Wall** → Transition to `WallRunning`
- **Roof Edge** → "Long Jump" boost if jump timed correctly

### Glide Mechanic

Reduce gravity constant significantly while glide key held, add constant forward force. Creates "controlled fall" for fluid navigation.

### Polish

| Effect | Purpose |
|--------|---------|
| FOV Warping | Increase FOV as velocity increases to simulate speed |
| Camera Shake | Subtle high-frequency shake at max sprint or hard landings |
| Animation Warping | Blend run → power-run with character leaning toward ground |

## Multiplayer Architecture

### The Problem

High-speed momentum physics + network latency = significant technical friction.

| Feature | Multiplayer Challenge |
|---------|----------------------|
| High Speed | Small lag spikes cause huge rubber-banding |
| Wall Running | Client thinks on wall; server thinks missed it |
| Momentum | Synchronizing exact velocity vectors across clients |
| Physics Objects | Who owns the car's position when it gets hit? |

### Solution: Client-Side Prediction with SpacetimeDB

Standard "send position every frame" networking won't work. Need rollback networking.

**Tech Stack:**
- Physics: `avian3d` (deterministic, easier to sync than Rapier)
- Networking: SpacetimeDB with `spacetime_physics`
- Client: `bevy_spacetimedb` bridge

### Prediction Loop

Maintain two versions of the player:

1. **Predicted Entity** — Player controls and sees. Responds instantly to input.
2. **Authoritative State** — Hidden data updated only on SpacetimeDB row updates (the "truth").

**Input Buffering:** Keep list of every input sent that hasn't been confirmed.

**Reconciliation:**
1. SpacetimeDB sends back actual position
2. Compare Predicted vs Authoritative
3. If mismatch: teleport Predicted to Authoritative, replay buffered inputs to catch up

### Determinism Requirement

Physics must be deterministic. If client calculates jump force as `9.810001` and server calculates `9.810002`, after 5 seconds of sprinting the player is in two different zip codes.

`spacetime_physics` is an XPBD solver built for SpacetimeDB modules—deterministic and Wasm-compatible. Run identical physics on client (Bevy) and server (SpacetimeDB module).

### Server Module Structure

```rust
use spacetime_physics::PhysicsWorld;

#[spacetimedb::table(name = physics_config, singleton)]
pub struct PhysicsConfig {
    pub world: PhysicsWorld,
}

#[spacetimedb::reducer(init)]
pub fn init_physics(ctx: &ReducerContext) {
    let world = PhysicsWorld::builder()
        .gravity(Vec3::new(0.0, -9.81, 0.0))
        .ticks_per_second(60.0)
        .build();

    ctx.db.physics_config().insert(PhysicsConfig { world });
}
```

### Critical: Build Multiplayer First

If multiplayer is added after finishing single-player movement, expect to rewrite ~80% of movement code. Movement logic must be built inside the networking rollback loop from Day 1.

### Smoothing Techniques

- **Client Authority for Movement:** Let client decide position; server sanity-checks (max speed validation)
- **Visual Interpolation:** Don't snap to server position—use spring/lerp to gently correct over frames
