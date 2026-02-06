# Vision: Data-Driven Game Systems

Status: **Working draft. Open for discussion. Nothing here is decided.**

## What We Want

A game where most behaviors are defined by composing small blocks in data files — not by writing custom Rust for each mechanic. LLMs and designers should be able to create new abilities, items, enemies, and interactions by mixing existing blocks.

The game uses Bevy (ECS). We want to actually leverage ECS: things in the world are entities with components, behaviors emerge from systems.

## The Approach We're Exploring

We've been exploring a system built on three primitives:

- **Expr** — a computed value (literal, stat reference, arithmetic over other Exprs)
- **Condition** — a boolean test composed from Exprs
- **Effect** — a state mutation composed from Exprs

A **Rule** (the first composition) bundles conditions with effects: "if X, then Y."

Rules are data (serializable, RON-loadable). They attach to entities as components and fire at different trigger points (on hit, on taking damage, each frame, etc.).

### Open questions about this model

- **Is this the right set of primitives?** SC2's data editor also uses validators (≈ Conditions) and effects, but SC2's "Effects" are much broader — they include spatial searches, projectile launches, entity spawning, and persistent area creation. Ours are narrow (set a number, trigger an event). Is our narrow definition better for composability, or does it push too much into hardcoded Rust systems?

- **Where does targeting/spatial logic live?** In SC2, "search this area for targets" is an Effect. In our current thinking, it's a system (Rust code) that reads properties off an entity. Which is more composable? Which is more ECS-native?

- **Are three primitives the right number?** Could Condition be an Expr that returns a boolean? Could Rule just be a pattern rather than a type? Are we splitting things correctly?

## Attacks as Entities

One thing we feel strongly about: attacks should be real entities in the world.

Currently, an "attack" is a boolean flag and timer on the player. It has no presence, no position, no shape, no visibility. This feels wrong.

An attack entity would have position, an area it affects, a visual, a lifetime. It would show up in debug tools. Multiple attacks could coexist. The visual effect IS the entity, not a separate thing spawned by an event.

### Open questions

- How much data lives on the attack entity vs. being read from the attacker at resolve time?
- Should the attack entity use physics colliders for hit detection, or manual spatial checks?
- How do attack properties (range, shape, timing) relate to character stats?

## Character Stats vs. Attack Properties

We believe these are different things:

- **Character stats** = who the entity is (health, strength, crit chance). Persistent. Apply to everything the character does.
- **Attack properties** = what a specific attack is (range, arc, knockback, timing). Belong to the attack or ability, not the character.

A character with two different abilities should have the same stats but different attack properties per ability.

## The Pipeline Question

When an attack resolves, some sequence happens: base values are set, rules modify them, targets are found, damage/force/feedback are applied. Currently this is one monolithic function.

### Open questions

- How much of this is "rules" (data-driven) vs. "pipeline" (Rust systems)?
- In our current model, rules set up values (Action context) and the pipeline applies them. Is this the right split? Or should more of the pipeline be expressible as rules?
- SC2 puts targeting into its effect system. We keep targeting in Rust. What are the trade-offs?

## Genre Considerations

SC2 is an RTS. Our game is an action RPG / roguelite. The differences matter:

- **RPGs need deep stat interactions.** Items, buffs, abilities all modify stats in layered ways. Our Expr/Condition/Effect model handles this well.
- **Roguelites need wild combinations.** Hades-style boons that chain and synergize. This needs effects that compose unpredictably. Is our model flexible enough?
- **Action RPGs need real-time feel.** Hit feedback, timing, spatial precision. This might need more from the Rust systems than from data-driven rules.
- **SC2 needs hundreds of unit types.** Its data editor optimizes for many similar-but-different units. We might need fewer types but deeper interactions.

## Reference Systems

Systems we should study and understand before committing:

- **StarCraft 2 Data Editor** — effect chains, validators, behaviors. Extremely powerful but notoriously complex.
- **Path of Exile skill gems** — skills composed from a base gem + support gems that modify behavior. Very modular.
- **Hades boons** — on-hit effects, procs, stacking. Great model for roguelite ability composition.
- **Unreal GAS** — Gameplay Ability System. Industry standard for ability/effect/attribute management.

**We haven't deeply studied all of these yet.** Before committing to our primitives, we should understand what these systems got right and wrong.

## The Long-Term Vision

The rule system should eventually be a general-purpose "game API" — not just combat. The same blocks that define "sword slash deals 25 damage" should be able to define AI behaviors, progression triggers, item effects, environmental interactions.

We don't need to build all of that now. But the primitives we choose shouldn't prevent it.

## What We're Unsure About

- Whether our three primitives (Expr, Condition, Effect) are the right foundation, or if we're missing something
- Whether Effect should stay narrow (data mutations only) or expand to include actions (deal damage, spawn entity, apply force)
- How much to put in data-driven rules vs. Rust systems
- The right level of abstraction — too low and everything is verbose, too high and you can't express edge cases
- How to make this powerful without making it impossible to understand
