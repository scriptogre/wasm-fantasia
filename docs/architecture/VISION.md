# Vision: Data-Driven Game Systems

Status: **Principles are stable. Implementation details are not. This doc captures direction, not spec.**

## What We Want

A game where most behaviors are defined by composing small building blocks — not by writing custom Rust for each mechanic. Designers and LLMs should be able to create new abilities, items, enemies, and interactions by mixing existing blocks.

The game uses Bevy (ECS). We leverage ECS fully: entities with components, behaviors emerging from systems.

## Core Insight: The Resolve Step

Every state change has three phases:

1. **Something is requested** — a player attacks, an item is used, a door is opened
2. **The request is validated and computed** — damage is calculated, rules/scripts evaluate, authority is checked
3. **Effects happen** — some change authoritative state (health, inventory), some are cosmetic (VFX, sound, screen shake)

The validation/computation step (phase 2) is the only code that differs between single-player and multiplayer. In single-player, it runs locally. In multiplayer, the server handles it. Everything before (input, requests) and after (effects, presentation) is shared.

State effects and cosmetic effects are siblings — parallel consequences of the same resolution, not sequential stages. This mirrors Unreal's Gameplay Ability System, where Gameplay Effects (state) and Gameplay Cues (cosmetics) are associated with the same outcome.

## Two Storages

A key architectural insight borrowed from action RPG design:

- **Stats** — persistent per-entity values. "Who you are." Health, attack damage, crit chance. Survives across actions.
- **Action context** — temporary per-action values. "What's happening right now." Computed damage, knockback, is_crit. Created fresh per action, modified by scripts/rules, consumed by the engine, then discarded.

A crit multiplier modifies the action's damage value, not the entity's attack damage stat. This separation prevents rules from accidentally corrupting persistent state and allows complex modifier stacking within a single action.

## Stats vs Attack Properties

These are different things:

- **Character stats** = who the entity is (health, strength, crit chance). Persistent. Apply to everything.
- **Attack properties** = what a specific attack is (range, arc, knockback, timing). Belong to the attack or ability, not the character.

A character with two abilities has the same stats but different attack properties per ability.

## Scripting: Lua

Rather than building a custom expression/condition/effect language in Rust, we plan to use embedded Lua for data-driven behaviors. Lua scripts work with building blocks exposed from Rust: stat storage, action context, trigger points, and an effects API.

What Lua replaces: hand-rolled Condition/Effect/Expr enums. What stays in Rust: stat storage, action context lifecycle, trigger dispatch, the effects API, hit detection, physics.

## Attacks as Entities

Attacks should be real entities in the world — not boolean flags and timers on the player.

An attack entity has position, area, visual, lifetime. It shows up in debug tools. Multiple attacks can coexist. The visual effect IS the entity, not a separate thing spawned after the fact.

Not yet implemented. The current architecture doesn't prevent this transition.

## Design Principles

1. **Everything composes.** Small blocks build into larger blocks.
2. **Two storages.** Stats = persistent. Action context = temporary. Don't conflate them.
3. **Scripts over custom code.** New behaviors via Lua, not new Rust systems.
4. **The resolve step is the seam.** Rules, multiplayer authority, and validation all live here.
5. **ECS-native.** Stats are components. Triggers are observers. Scripting layers on top of ECS, not beside it.
6. **Compose before coding.** Can existing blocks do this? If not, what's the smallest new block?

## Reference Systems

- **Unreal GAS** — Gameplay Ability System. Closest to what we're building. Effect/Cue separation, attribute system, ability activation flow.
- **StarCraft 2 Data Editor** — effect chains, validators, behaviors. Powerful but notoriously complex.
- **Path of Exile skill gems** — base gem + support gems modifying behavior. Modular composition.
- **Hades boons** — on-hit procs, stacking, synergies. Great roguelite model.

## Open Questions

- How much logic lives in Lua vs Rust? (Targeting? Spatial logic? AI?)
- What's the right Lua API surface? Too narrow and scripts can't do anything. Too wide and they bypass the resolve step.
- How do attack entities interact with scripting? Do scripts live on the attack entity or the character?
- Should behaviors (buffs/debuffs with duration and stacking) be a Lua concept or a Rust framework?
- What's the exact naming convention for events? We have strong instincts (past tense for outcomes, noun form for requests) but haven't fully committed.
- How much of Unreal GAS's model applies to an ECS game with Lua scripting?
