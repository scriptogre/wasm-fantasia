# Game Architecture Patterns: Research & Reference

Status: **Reference document. Patterns extracted from shipped games and production engines.**

## 1. Overwatch ECS (Blizzard, GDC 2017)

Source: Timothy Ford, "Overwatch Gameplay Architecture and Netcode" — [GDC Vault](https://www.gdcvault.com/play/1024001/-Overwatch-Gameplay-Architecture-and)

The most directly relevant reference for an ECS game with client/server multiplayer.

### Iron Rules

| Rule | Rationale |
|------|-----------|
| Components have no functions | Pure data. Serializable, snapshotable, rollbackable. |
| Systems have no state | No member variables. All state lives in components. |
| Shared code lives in Utils | Read-only utility functions outside any system. |
| Systems cannot call other systems | Communication only through component mutation. |

### Singleton Components (~40% of all components)

Global state that many systems read: keyboard input, game clock, match state. Instead of injecting services or using globals, they store unique state as a component on a dedicated entity. In Bevy terms: `Res<T>` / `ResMut<T>` resources.

**Relevance to us**: We already use Bevy resources for this. Validates the pattern.

### Deferred Side Effects

The most important Overwatch pattern for us. When a system detects something (projectile hits target), it does NOT immediately spawn VFX, play sounds, or apply damage. Instead:

1. System writes to a **queue component** (e.g., `ModifyHealthQueue`)
2. A dedicated **processing system** runs later in the frame, drains the queue, applies effects
3. Cosmetic systems (VFX, sound, screen shake) read the processed results

Why: During projectile simulation, you don't want to spawn particles mid-loop. Batching prevents cascade bugs, enables ordering control, and makes rollback trivial — just discard the queue.

**Relevance to us**: Our combat resolution already uses `resolve_combat()` as a shared function. The queue pattern extends this — combat events should flow through an explicit event queue that cosmetic systems subscribe to, rather than triggering effects inline.

### Rollback-Friendly ECS

Because components are pure data, snapshots are trivial: copy the component arrays. Rollback = restore arrays + re-simulate from the divergence point. Only components tagged as "predicted" need rollback. Movement state and damage state are separate components, enabling independent rollback granularity.

### Network Time Compression

When the server detects network instability, it tells the client to speed up slightly (60fps -> 65fps). This generates extra input frames without increasing packet size. The server buffers client input, absorbing jitter. Overwatch quantizes time into "Command Frames" (fixed 16ms ticks in competitive).

---

## 2. Quake 3 / id Tech 3 (id Software)

Source: [Fabien Sanglard's Quake 3 Source Code Review](https://fabiensanglard.net/quake3/)

### Enforced Client-Server Split

John Carmack: "The explicit split of networking into a client presentation side and the server logical side was really the right thing to do."

The architecture compiles game logic into three separate virtual machines:
- `game` — server-side logic (authoritative)
- `cgame` — client-side prediction and presentation
- `q3_ui` — interface

Communication between engine and VMs uses message passing (`VM_Call()` + syscalls), not function calls. This is the most extreme separation possible — the game logic literally cannot access engine internals directly.

**Key insight**: id Tech 3 is "a mini operating system providing system calls to three processes." The VMs represent ~30% of the codebase.

### Unified Event Queue

All inputs (keyboard, mouse, network packets) convert to standardized `event_t` structs placed in a single centralized queue (`sysEvent_t eventQue[256]`). This enables input recording, replay, and deterministic debugging.

**Relevance to us**: Our `bevy_enhanced_input` system already abstracts input. But the principle extends — all external stimuli should funnel through a single normalized channel before reaching game logic.

### Data-Driven Rendering

The 3D engine has two completely separate code paths: world geometry and entities. The world is static BSP data. Entities are dynamic. No shared rendering code between them.

---

## 3. Source Engine (Valve)

Source: [Valve Developer Community](https://developer.valvesoftware.com/wiki/Engine_Structure), [Source SDK 2013 DeepWiki](https://deepwiki.com/ValveSoftware/source-sdk-2013)

### Three-DLL Architecture

| DLL | Purpose |
|-----|---------|
| `engine.dll` | Core systems: rendering, physics, networking, audio |
| `server.dll` | Authoritative game logic, entity simulation |
| `client.dll` | Prediction, interpolation, UI, client-side effects |

Game-specific code lives in `server.dll` and `client.dll`. Engine code is shared infrastructure. This is the same split we have (client/shared/server crates).

### Shared Code via Conditional Compilation

The same source files compile into both `client.dll` and `server.dll`. Client code is guarded by `#define CLIENT_DLL`, server code by `#define GAME_DLL`. Shared logic compiles for both targets without duplication.

**Relevance to us**: Our `shared/` crate serves this purpose. The Source Engine validates the pattern of a shared crate containing pure logic (combat resolution, rules, RNG) with no rendering or IO dependencies.

### CTFPlayerShared — Component Pattern in OOP

Even in an inheritance-heavy engine, Valve uses composition. `CTFPlayerShared` attaches to player entities and handles buffs, disguises, and class-specific abilities without deep inheritance hierarchies. Behaviors compose through attached subsystems rather than class derivation.

---

## 4. Flecs Design Patterns (Sander Mertens)

Source: [Designing with Flecs](https://www.flecs.dev/flecs/md_docs_2DesignWithFlecs.html), [ECS FAQ](https://github.com/SanderMertens/ecs-faq)

The most comprehensive ECS-specific architectural guide available. Not a game engine itself, but the patterns apply directly to Bevy.

### Atomic Components

Split `Transform { position, rotation, scale }` into `Position`, `Rotation`, `Scale`. Benefits:
- Systems query only what they need (cache efficiency)
- Broader entity compatibility (not everything that moves also rotates)
- Smaller archetype tables

**Caveat for Bevy**: Bevy's `Transform` is deliberately monolithic for ergonomics and because the engine needs all three for rendering. This pattern applies more to game-specific components. Don't split `Health` and `MaxHealth` into separate components — they're always accessed together.

### Frame Phase Convention

| Phase | Purpose | Example |
|-------|---------|---------|
| OnLoad | Import external data | Read input, sensor data |
| PostLoad | Process into game actions | Map raw input to commands |
| PreUpdate | Prepare the frame | Reset per-frame state |
| OnUpdate | Gameplay logic | Movement, combat, AI |
| OnValidate | Detection and validation | Collision detection |
| PostUpdate | Apply corrections | Resolve penetrations |
| PreStore | Prepare for rendering | Interpolation |
| OnStore | Render | Submit draw calls |

**Relevance to us**: Our `PostPhysicsAppSystems` set already defines: `UserInput -> TickTimers -> ChangeUi -> PlaySounds -> PlayAnimations -> Update`. This aligns with the phase model. Consider whether we need an explicit "Validate" phase between gameplay and presentation.

### Organize by Feature, Not by Type

Structure modules around complete features (combat, physics, movement), not around ECS primitives (all components in one file, all systems in another). Each feature module exports its own components and systems.

**Relevance to us**: We already do this (combat/, player/, camera/ etc.). Validated.

### Separate Component Definitions from System Implementations

Create distinct `components.rs` and `systems.rs` within each feature module. Consumers can import component types without pulling in system logic. Enables swapping implementations (e.g., test mocks) without changing data structures.

### Relationships for Entity Associations

Use ECS relationships (parent-child, custom) instead of storing `Entity` handles in components when:
- You need to find all entities referencing a specific entity (reverse lookup)
- You're modeling containers (inventories, party members)
- You need to group entities by attribute (spatial cells, teams)

---

## 5. Bevy-Specific Best Practices

Source: [Bevy Best Practices](https://github.com/tbillington/bevy_best_practices), [Tainted Coders](https://taintedcoders.com/bevy/code-organization)

### Events for Subsystem Communication

`EventWriter`/`EventReader` are thin `Vec` wrappers — nearly zero cost. Use them aggressively to decouple gameplay, audio, VFX, and UI systems. Multiple readers can run in parallel; single writer keeps consistency.

Place event readers **after** writers within the same frame. Use `.chain()` or `before()`/`after()` for systems in the same SystemSet.

Systems that only read events should use `run_if(on_event::<T>())` to avoid running when no events exist.

### Every Update System Needs a Run Condition

All systems in `Update` should have both:
1. A state-based run condition (`in_state(GameState::Playing)`)
2. A SystemSet for ordering (`in_set(UpdateSet::Combat)`)

This prevents systems from executing during wrong game states and makes ordering explicit rather than implicit.

### Strong IDs Over Entity Handles for Persistence

Entity handles are frame-local pointers. For anything that survives across saves, network sync, or session boundaries, create custom ID types with private constructors and Display/Serialize implementations.

**Relevance to us**: Critical for multiplayer. SpacetimeDB entities need stable IDs that map to Bevy entities, not raw `Entity` handles.

### Cleanup via State-Scoped Entities

Use `StateScoped(GameState::Playing)` on top-level entities for automatic despawn on state exit. Child entities inherit parent cleanup. Eliminates manual cleanup system boilerplate.

### Minimal main.rs

`main.rs` should contain only `App::new().add_plugins(GamePlugin).run()`. All logic lives in `lib.rs` and sub-modules. This enables test harnesses that spin up the app without rendering.

---

## 6. Game Programming Patterns (Bob Nystrom)

Source: [gameprogrammingpatterns.com](https://gameprogrammingpatterns.com/contents.html)

The patterns most relevant to ECS game architecture:

### Event Queue (Decoupling Pattern)

Decouple event producers from consumers via a queue. The sender fires-and-forgets. The consumer processes at its own pace. Critical for: audio triggers, VFX spawning, analytics, achievement tracking.

Use a secondary event queue for non-essential side effects. Emit events where the effect should occur, process them later in the frame. This is exactly the Overwatch "deferred side effects" pattern.

### Command (Revisited Pattern)

Encapsulate requests as objects. In ECS terms: combat actions, ability activations, and movement requests should be data (components or events), not function calls. This enables: undo/redo, replay, network serialization, input buffering.

**Relevance to us**: Our shared `resolve_combat()` already treats combat as data. Extend this — all state-changing actions should be serializable command objects.

### Dirty Flag (Optimization Pattern)

Track whether derived data needs recalculation. In Bevy: `Changed<T>` and `Added<T>` query filters. Don't recompute stat totals every frame — only when a stat modifier changes.

**Relevance to us**: Bevy's change detection is built-in. Use `Changed<Stats>` to trigger recalculation systems rather than running every frame.

### Data Locality (Optimization Pattern)

Arrange data for cache efficiency. ECS does this by default via archetype storage. But be aware: adding rare components to common archetypes fragments the table. Use sparse-set storage (`#[component(storage = "SparseSet")]`) for components that are added/removed frequently.

---

## 7. Cross-Cutting Patterns for Client/Server ECS

Synthesized from all sources above. These are the patterns most relevant to our specific architecture (Bevy client + SpacetimeDB server + shared crate).

### The Three-Layer Command Flow

```
Input Layer (client only)
    → raw input → mapped actions → command events

Resolve Layer (shared, runs on both client and server)
    → validate command → compute outcome → produce state changes

Presentation Layer (client only)
    → read state changes → spawn VFX, play sounds, update UI
```

The resolve layer is the **only** code that must be identical between client and server. Input handling is client-only. Presentation is client-only. The shared crate contains resolve logic and nothing else.

### Component Categories for Multiplayer

| Category | Replicated? | Predicted? | Example |
|----------|------------|------------|---------|
| Authoritative State | Yes | Sometimes | Health, Position, Inventory |
| Cosmetic State | No | No | Particle timers, animation blend |
| Input State | Client→Server | Yes | Movement intent, attack request |
| Derived State | No (recomputed) | No | Computed stat totals, UI display values |

Tag components with their replication category. Not every component needs to cross the network. Cosmetic and derived state should never be replicated.

### Event Taxonomy

| Tense | Meaning | Example | Who handles |
|-------|---------|---------|-------------|
| Request (imperative) | "I want to do X" | `AttackRequest` | Validation system |
| Outcome (past tense) | "X happened" | `DamageDealt` | Cosmetic systems |
| State change | "X is now Y" | `HealthChanged` | UI, networking |

Requests flow inward (input → logic). Outcomes flow outward (logic → presentation). Never let presentation systems generate requests directly.

---

## Key Takeaways Ranked by Relevance

1. **Deferred side effects via event queues** (Overwatch) — Most impactful pattern we're not fully using. Combat events should queue, not trigger inline effects.
2. **Component categories for multiplayer** (synthesized) — Tag every component by replication intent. Prevents over-syncing.
3. **Request/Outcome event taxonomy** (synthesized) — Codify the naming and directional flow of events.
4. **Shared resolve layer as the only duplicated code** (Quake 3, Source Engine, our VISION.md) — We're already doing this. Stay disciplined.
5. **Frame phases with explicit ordering** (Flecs, Bevy best practices) — Our SystemSets are good. Consider adding a Validate phase.
6. **Singleton components for global state** (Overwatch) — Already using Bevy resources. Validated.
7. **Separate component definitions from system implementations** (Flecs) — Worth adopting within feature modules.
8. **Strong IDs for networked entities** (Bevy best practices) — Required for SpacetimeDB sync.

## References

- [Overwatch Gameplay Architecture and Netcode (GDC 2017)](https://www.gdcvault.com/play/1024001/-Overwatch-Gameplay-Architecture-and)
- [Overwatch ECS Architecture Details](https://topic.alibabacloud.com/a/on-the-ecs-architecture-in-the-overwatch_8_8_31063753.html)
- [Quake 3 Source Code Review — Fabien Sanglard](https://fabiensanglard.net/quake3/)
- [Source SDK 2013 Architecture](https://deepwiki.com/ValveSoftware/source-sdk-2013)
- [Designing with Flecs](https://www.flecs.dev/flecs/md_docs_2DesignWithFlecs.html)
- [ECS FAQ — Sander Mertens](https://github.com/SanderMertens/ecs-faq)
- [Bevy Best Practices](https://github.com/tbillington/bevy_best_practices)
- [Bevy Code Organization — Tainted Coders](https://taintedcoders.com/bevy/code-organization)
- [Game Programming Patterns — Bob Nystrom](https://gameprogrammingpatterns.com/contents.html)
- [ECS Design Decisions — Ariel Coppes](https://arielcoppes.dev/2023/07/13/design-decisions-when-building-games-using-ecs.html)
- [Godot: Why Not ECS](https://godotengine.org/article/why-isnt-godot-ecs-based-game-engine/)
- [Celeste Physics Architecture — Maddy Thorson](https://maddythorson.medium.com/celeste-and-towerfall-physics-d24bd2ae0fc5)
