# Decisions

Architectural decisions that survive between sessions. Code is the source of truth for implementation; this file captures the *why* and *future intent*.

## Server Physics: Fork Avian3d

**Decision:** When server-side physics is needed (MOVEMENT.md, NPC knockback), fork avian3d and extract a `avian3d-core` crate with no Bevy dependency. Both client (Bevy + avian3d) and server (SpacetimeDB + avian3d-core) run identical physics.

**Rationale:** Running two different physics engines (avian3d on client, rapier3d on server) creates behavioral drift and code duplication. Avian's author confirmed the core solver *could* be decoupled from Bevy (GitHub issue #748). The Bevy coupling is in system scheduling and component storage, not in the math.

**Status:** Deferred. Using minimal knockback physics on server (`pos += vel * dt`) until this work begins. The combat service layer (`resolve_combat()`) returns forces — how the server applies them is replaceable.

## Combat Service Layer

**Decision:** All combat resolution logic lives in `shared/src/combat.rs` as pure functions. Both client and server call `resolve_combat()` with their data, apply results to their storage. Zero duplicated game logic.

**Status:** Implementing.

## Scripting: Lua over Custom Language

**Decision:** Use embedded Lua for data-driven behaviors instead of building a custom Expr/Condition/Effect language in Rust. See `docs/architecture/VISION.md`.

**Status:** Not started. Current rules system uses Rust enums as an interim.
