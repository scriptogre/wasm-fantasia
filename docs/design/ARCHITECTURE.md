# Architecture

## Design philosophy

**Data-driven composition.** Designers and LLMs create new abilities, items, enemies, and environments by composing existing building blocks — not by writing new Rust systems. Rune scripting handles behavior logic; Rust handles infrastructure (stats, physics, hit detection, rendering).

**Environment as data.** Environment visuals are driven by gameplay state, not hand-authored scene variants. Wave number, corruption level, biome health — stored as data, read by visual systems that drive material parameters, fog, lighting, particle spawning. Scriptable via Rune like any other game behavior. One parameterized world, not multiple bespoke scenes.

**Everything composes.** Small blocks build into larger blocks. The same scripting API that defines abilities also defines environment transitions and enemy behaviors.

## Scripting runtime

Rune scripts mutate cloned `Combatant` snapshots through semantic primitives (`apply_damage`, `heal`, `set_stat`), so a script can read the result of its own writes. Each primitive logs an `Intent`; presentation calls (`vfx`, sound) log an `Effect`. After the script returns, the engine applies intents to real state (ECS on client, SpacetimeDB tables on server) and the client drains effects. Server runs no-op effect bodies.

## Multiplayer model

SpacetimeDB is the authority for all game modes. Singleplayer = local SpacetimeDB instance. The shared `core/` crate contains pure logic (combat resolution, rules, RNG) with no Bevy or IO dependencies.
