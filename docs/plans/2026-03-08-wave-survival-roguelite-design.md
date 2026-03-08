# Wave Survival Roguelite Loop — Design

## Overview

Transform the current flat-arena combat prototype into a playable roguelite survival mode. Continuous escalating horde, XP orbs, level-up upgrade picks, stackable upgrades with breakpoint mutations, death-and-retry loop. No meta-progression yet — that's a future layer.

## Core Loop

```
Spawn → Fight continuous horde → Enemies drop XP orbs → Walk over orbs →
XP bar fills → Level up → Game pauses → Pick 1 of 3 upgrades → Resume →
Get stronger → Horde escalates → Eventually die → Score screen → Try again
```

## Systems

### 1. Horde Spawner

Replaces E-key manual spawning. Enemies spawn continuously in a ring around the player (outside camera view).

**Escalation curve:**
- Spawn rate starts at ~1 enemy/sec, ramps over time
- Mix shifts as time progresses:
  - 0–60s: Basic zombies only
  - 60–120s: Introduce Fast zombies (30% of spawns)
  - 120–180s: Introduce Brute zombies (20% of spawns)
  - 180s+: All types, density keeps climbing
- No upper cap — the game ends when you die

**Enemy variants (minimal, same model with tint/scale tweaks):**

| Variant | HP | Speed | Damage | Trait |
|---------|-----|-------|--------|-------|
| Basic | 500 | 2.0 m/s | 10 | — |
| Fast | 200 | 4.0 m/s | 10 | — |
| Brute | 1200 | 1.2 m/s | 25 | Knockback resistant |

### 2. XP Orbs

- Enemies drop a physical XP orb entity on death
- Orbs have a pickup radius (~1.5m) — player walks near them to collect
- Orb value scales with enemy type (Basic: 10, Fast: 8, Brute: 25)
- Orbs persist on the ground (no despawn timer for now)
- Visual: small glowing sphere, drifts toward player when within pickup radius

### 3. Leveling

- Player has current XP, current level, and XP-to-next-level
- XP required per level: `100 * 1.15^level` (starts at 100, grows ~15% per level)
- No level cap
- On level up: trigger upgrade selection

### 4. Upgrade Selection (Level-Up)

- Game pauses (time scale → 0)
- Overlay appears with 3 upgrade cards
- Each card shows: name, short description, current stack count (if already owned), next breakpoint preview
- Player clicks one → upgrade applied → game resumes
- Cards are drawn from the full upgrade pool (including upgrades you already have — picking a duplicate adds a stack)

### 5. Upgrade Primitives

The upgrade system is built on three layers of composable primitives.

#### Layer 1: Events (hooks in Rune scripts)

| Event | Fires when |
|-------|-----------|
| `on_hit` | Attack connects with enemy |
| `on_crit` | Critical hit specifically |
| `on_kill` | Enemy dies from this attack |
| `on_damaged` | Player takes damage |
| `on_dodge` | Player rolls/dodges |
| `on_ground_pound` | Ground pound lands |

#### Layer 2: Status Effects (conditions applied to enemies)

4 orthogonal statuses covering damage, control, amplification, disruption:

| Status | Effect | Duration |
|--------|--------|----------|
| **Burn** | X damage/sec, stacks intensity | 3s, refreshes on reapply |
| **Slow** | Reduce move speed by X% | 2s, refreshes on reapply |
| **Mark** | Take +50% damage from all sources | 3s, refreshes on reapply |
| **Stagger** | Stunned, cannot act | 0.3s, does not stack duration |

Why these 4:
- Burn = damage over time
- Slow = movement control
- Mark = damage amplification
- Stagger = action denial
- Every pair composes meaningfully (Burn+Slow = can't escape DoT, Mark+anything = amplified, etc.)
- Orthogonal — no overlap between them

#### Layer 3: Attack Modifiers (how damage is delivered)

| Modifier | Effect |
|----------|--------|
| **Chain(n)** | Hit arcs to n nearby enemies |
| **Pierce** | Attack passes through to enemies behind |
| **Area(r)** | Splash damage in radius around impact |
| **Leech(%)** | Heal % of damage dealt |

### 6. Upgrade Stacking & Breakpoints

Every upgrade can be picked multiple times. Each stack provides a linear stat increase. At breakpoints (3, 5, 10 stacks), the upgrade mutates — gaining new behavior.

#### Starter Upgrades (initial pool of ~8)

**Inferno** (Burn tree)
- 1 stack: on_hit → 30% chance to apply Burn (5 dps)
- Per stack: +5 dps, +5% chance
- 3 stacks: Burn damage ticks faster (2×)
- 5 stacks: Burn spreads to 1 nearby enemy on tick
- 10 stacks: Burning enemies explode on death (Area damage)

**Permafrost** (Slow tree)
- 1 stack: on_hit → 25% chance to apply Slow (20%)
- Per stack: +5% slow intensity, +5% chance
- 3 stacks: Slowed enemies take +15% damage
- 5 stacks: Slow applies to all enemies in 3m radius of target
- 10 stacks: Fully slowed enemies (100%) freeze solid for 1s

**Hunter's Eye** (Mark tree)
- 1 stack: on_crit → apply Mark to target
- Per stack: +10% Mark damage amp
- 3 stacks: Marked enemies have -20% attack speed
- 5 stacks: Killing a Marked enemy resets crit cooldown
- 10 stacks: Mark spreads to 3 nearby enemies on application

**Concussion** (Stagger tree)
- 1 stack: on_hit → 10% chance to Stagger
- Per stack: +0.05s stagger duration, +3% chance
- 3 stacks: Staggered enemies take 2× crit damage
- 5 stacks: Ground pound always Staggers in radius
- 10 stacks: Stagger triggers a shockwave that Staggers nearby enemies

**Arc Conductor** (Chain tree)
- 1 stack: on_hit → 20% chance to Chain(1)
- Per stack: +1 chain target, +5% chance
- 3 stacks: Chains apply all your status effects
- 5 stacks: Chain range doubled
- 10 stacks: Chains can bounce back to already-hit targets

**Impaler** (Pierce tree)
- 1 stack: on_kill → next attack Pierces
- Per stack: +15% Pierce damage
- 3 stacks: Pierce also applies knockback
- 5 stacks: Pierced enemies leave a damage trail
- 10 stacks: Pierce has no target limit (infinite pass-through)

**Vampiric Strikes** (Leech tree)
- 1 stack: Leech(5%) on all damage
- Per stack: +3% Leech
- 3 stacks: Overhealing grants temporary shield
- 5 stacks: Leech applies to DoT (Burn)
- 10 stacks: When above 90% HP, +25% damage

**Aftershock** (Area tree)
- 1 stack: on_kill → Area(2m) damage burst (50% of kill damage)
- Per stack: +0.5m radius, +10% damage
- 3 stacks: Area burst applies Slow
- 5 stacks: Ground pound Area doubled
- 10 stacks: Area bursts can trigger other Area bursts (chain reaction)

### 7. How Upgrades Map to Rune Scripts

Each upgrade is a Rune behavior script with a `stack_count` parameter. The script registers hooks and checks stack thresholds:

```
// Pseudocode for Inferno upgrade
fn on_hit(ctx, stack_count) {
    let chance = 0.30 + (stack_count - 1) * 0.05;
    let dps = 5.0 + (stack_count - 1) * 5.0;
    let tick_rate = if stack_count >= 3 { 2.0 } else { 1.0 };

    if rand() < chance {
        apply_status(ctx.target, Burn { dps, tick_rate, duration: 3.0 });
    }

    if stack_count >= 5 {
        // Burn spreads on tick — registered separately
    }
}

fn on_kill(ctx, stack_count) {
    if stack_count >= 10 && ctx.target.has_status(Burn) {
        area_damage(ctx.target.position, radius: 4.0, damage: ctx.target.burn_dps * 3.0);
    }
}
```

This is the exact surface where LLM-generated scripts plug in later — same hook system, same status/modifier vocabulary, new creative combinations.

### 8. Death & Score Screen

- Player HP reaches 0 → death animation → 1s pause → score overlay
- Score screen displays:
  - Time survived
  - Enemies killed
  - Level reached
  - Upgrades taken (with stack counts)
- "Try Again" button → full reset, new run
- No meta-progression between runs (future work)

### 9. UI Additions

| Element | Location | Description |
|---------|----------|-------------|
| XP bar | Bottom center | Horizontal bar, fills left→right, shows current/needed |
| Level indicator | Left of XP bar | Current level number |
| Kill counter | Top right corner | Running kill count |
| Upgrade overlay | Center screen | 3 cards on level-up, click to pick |
| Active upgrades | Left edge | Small icons with stack count for each held upgrade |
| Score screen | Full overlay | On death, stats + retry button |

### 10. What This Does NOT Include

- Permanent meta-progression between runs
- Weapon switching / multiple weapons
- Weapon aspects / branching weapon evolution
- Bosses or elite enemies
- Map variety / procedural levels
- LLM-generated upgrades (future — uses the same Rune primitive system)
- Sound design / music
- Visual effects for statuses (minimal placeholder tints only)

These are all future layers that build on top of this foundation.

## Architecture Notes

### New Components
- `XpOrb` — marker component for orb entities
- `PlayerXp { current, level, to_next }` — player resource
- `HordeSpawner { elapsed, spawn_rate, ... }` — spawner resource
- `UpgradeInventory` — maps upgrade ID → stack count
- `StatusEffects` — component on enemies, maps status → (intensity, remaining_duration)
- `ActiveUpgrades` — ordered list of held upgrades for UI display

### Server Authority
- XP, leveling, and upgrade selection must be server-authoritative
- Orb spawning and pickup validated server-side
- Upgrade effects run on both client (prediction) and server (authority) via Rune scripts
- Horde spawner runs server-side, syncs enemy spawns to clients

### Upgrade Selection Flow
1. Server detects level-up → sends `LevelUp` event to client
2. Server generates 3 random upgrade options (seeded RNG) → sends to client
3. Client pauses, shows overlay
4. Client sends selection back to server
5. Server validates, applies upgrade, sends confirmation
6. Client resumes
