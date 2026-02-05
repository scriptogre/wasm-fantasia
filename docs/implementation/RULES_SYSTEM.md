# Rules System

Data-driven reactive behaviors for game entities. Rules let you define "when X happens, do Y" without writing Rust code.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        BUILDING BLOCKS                          │
├─────────────────────────────────────────────────────────────────┤
│  Triggers (WHEN)     │  Conditions (IF)    │  Effects (WHAT)    │
│  ─────────────────   │  ────────────────   │  ───────────────   │
│  OnPreHitRules       │  Chance(0.2)        │  Low: Set, Modify  │
│  OnHitRules          │  Gt(Stat, val)      │  High: Damage,     │
│  OnCritHitRules      │  Lt(Stat, val)      │    Knockback,      │
│  OnKillRules         │  ChanceFrom(Stat)   │    Launch, Pull    │
│  OnTimerRules        │                     │                    │
└─────────────────────────────────────────────────────────────────┘
```

## Block Types

### Triggers - WHEN rules execute

| Trigger | Fires When | Use Case |
|---------|------------|----------|
| `OnPreHitRules` | Before hit resolves | Compute damage, determine crit |
| `OnHitRules` | After any hit lands | Gain stacks, lifesteal |
| `OnCritHitRules` | After a critical hit (derived) | Bonus effects on crit |
| `OnKillRules` | After killing an enemy | Execute procs, reset cooldowns |
| `OnTimerRules` | Periodically | Stack decay, DOT ticks |

**Derived triggers**: `OnCritHitRules` fires automatically when `HitIsCrit > 0.5`. You don't manually emit crit events.

### Conditions - IF effects should apply

```rust
Condition::Chance(0.2)              // 20% random chance
Condition::ChanceFrom(Stat::CritRate) // Chance from stat value
Condition::Gt(Stat::Stacks, 5.0)    // If stacks > 5
Condition::Lt(Stat::Health, 0.3)    // If health < 30%
Condition::Gte(Stat::Stacks, 10.0)  // If stacks >= 10
Condition::Eq(Stat::HitIsCrit, 1.0) // If this hit is a crit
```

### Effects - WHAT happens

Effects have two levels of abstraction.

#### Low-Level Effects (building blocks)

Full control over any stat. Use for stats that don't have high-level shortcuts.

```rust
// Set a stat to exact value
Effect::Set { stat: Stat::HitIsCrit, value: 1.0 }

// Modify a stat with a literal value
Effect::Modify { stat: Stat::Stacks, op: Op::Add, value: 1.0 }
Effect::Modify { stat: Stat::CritRate, op: Op::Multiply, value: 1.5 }

// Modify a stat using another stat's value
Effect::ModifyFrom { stat: Stat::Inactivity, op: Op::Subtract, from: Stat::DeltaTime }
// Inactivity = Inactivity - DeltaTime

// Debug logging
Effect::Log("Something happened!".into())
```

#### High-Level Effects (semantic shortcuts)

Readable shortcuts for common combat stats. **All take (Op, value)** so you can set, multiply, or add.

```rust
// Damage
Effect::Damage(Op::Set, 25.0)       // Set damage to 25
Effect::Damage(Op::Multiply, 2.5)   // Multiply damage by 2.5
Effect::Damage(Op::Add, 10.0)       // Add 10 to damage

// Force (knockback, pull, launch, etc.)
Effect::Knockback(Op::Set, 5.0)     // Set knockback to 5
Effect::Knockback(Op::Multiply, 2.0) // Double the knockback
Effect::Pull(Op::Set, 3.0)          // Pull toward attacker with force 3
Effect::Launch(Op::Set, 8.0)        // Launch upward with force 8
Effect::Slam(Op::Set, 5.0)          // Slam downward with force 5
Effect::Push(Op::Set, 4.0)          // Push in attacker's facing direction
```

#### When to Use Low-Level vs High-Level

| Scenario | Use | Example |
|----------|-----|---------|
| Damage or force effects | **High-level** | `Effect::Damage(Op::Multiply, 2.5)` |
| Stats without shortcuts | **Low-level** | `Effect::Modify { stat: Stat::Stacks, op: Op::Add, value: 1.0 }` |
| Setting HitIsCrit flag | **Low-level** | `Effect::Set { stat: Stat::HitIsCrit, value: 1.0 }` |

**Rule of thumb**: Use high-level for damage/force (semantic meaning), low-level for everything else.

## Stats (RuleVars)

Stats are stored per-entity in `RuleVars`. All values are f32.

### System-Provided Stats
```rust
Stat::DeltaTime       // Frame delta in seconds (set by tick system)
```

### Persistent Stats (entity state)
```rust
Stat::Stacks          // Combat stacks (attack speed buff)
Stat::Health          // Current health
Stat::MaxHealth       // Maximum health
Stat::CritRate        // Crit chance (0.0 to 1.0)
Stat::CritMultiplier  // Crit damage multiplier
Stat::AttackPower     // Base attack power
Stat::Inactivity      // Countdown for inactivity detection
```

### Computed Stats (per-hit, set by rules)
```rust
Stat::HitDamage        // Damage of current hit
Stat::HitIsCrit        // 1.0 if crit, 0.0 otherwise
Stat::HitForceRadial   // Force away from attacker (negative = pull)
Stat::HitForceForward  // Force in attacker's facing direction
Stat::HitForceVertical // Force along world Y (positive = up)
```

### Countdown Pattern

Countdowns are just stats decremented using `ModifyFrom` with `DeltaTime`:

```rust
// Reset countdown on hit
OnHitRules(vec![Rule::new()
    .with_effect(Effect::Set { stat: Stat::Inactivity, value: 2.5 })
])

// Decrement each frame, trigger when expired
OnTickRules(vec![
    Rule::new()
        .with_condition(Condition::Gt(Stat::Inactivity, 0.0))
        .with_effect(Effect::ModifyFrom {
            stat: Stat::Inactivity,
            op: Op::Subtract,
            from: Stat::DeltaTime,
        }),
    Rule::new()
        .with_condition(Condition::Lte(Stat::Inactivity, 0.0))
        .with_effect(/* triggered on expiry */),
])
```

No magic timer types - just composing existing building blocks.

## Force System

Forces are computed from three orthogonal components, translated to world space by the combat system:

```
                    Vertical (+Y)
                        ↑  Launch
                        │
                        │
        Pull ←──────────┼──────────→ Knockback
      (Radial-)         │           (Radial+)
                        │
                        ↓  Slam
                    Vertical (-Y)

        Forward: Attacker's facing direction (independent axis)
```

The system computes direction vectors from combat geometry:
- **Radial**: `normalize(target_pos - attacker_pos)` (horizontal)
- **Forward**: Attacker's facing direction (horizontal)
- **Vertical**: World Y axis

Final force = `radial_dir × HitForceRadial + forward_dir × HitForceForward + Y × HitForceVertical`

### Force Examples

| Attack Type | Effect | Result |
|-------------|--------|--------|
| Basic punch | `Knockback(Op::Set, 3.0)` | Push away |
| Uppercut | `Knockback(Op::Set, 1.0)` + `Launch(Op::Set, 6.0)` | Launch + slight push |
| Charge | `Push(Op::Set, 8.0)` | Push in attack direction |
| Vacuum | `Pull(Op::Set, 5.0)` | Pull toward attacker |
| Crit modifier | `Knockback(Op::Multiply, 2.5)` | Double+ the knockback |

## Event Flow

```
Attack button pressed
        ↓
tick_attack_state (system)
        ↓ (at hit_time)
AttackConnect event
        ↓
┌─────────────────────────────────┐
│ System sets base values:        │
│   HitDamage = 25.0              │
│   HitForceRadial = 3.0          │
│   HitForceForward = 0.0         │
│   HitForceVertical = 0.0        │
│   HitIsCrit = 0.0               │
└─────────────────────────────────┘
        ↓
OnPreHitRules execute (may modify values)
        ↓
System computes force vector, triggers DamageEvent
        ↓
OnHitRules execute
        ↓
If HitIsCrit > 0.5 → OnCritHitRules execute
        ↓
If target died → OnKillRules execute
```

## Complete Examples

### Crit System

```rust
RuleVars::new()
    .with(Stat::CritRate, 0.20),  // 20% crit chance

OnPreHitRules(vec![
    Rule::new()
        .with_condition(Condition::ChanceFrom(Stat::CritRate))
        .with_effect(Effect::Set { stat: Stat::HitIsCrit, value: 1.0 })
        .with_effect(Effect::Damage(Op::Multiply, 2.5))
        .with_effect(Effect::Knockback(Op::Multiply, 2.5)),
]),
```

### Stacking Attack Speed

```rust
// Gain stack on hit, reset inactivity countdown
OnHitRules(vec![
    Rule::new()
        .with_effect(Effect::Modify {
            stat: Stat::Stacks,
            op: Op::Add,
            value: 1.0,
        })
        .with_effect(Effect::Set {
            stat: Stat::Inactivity,
            value: 2.5,
        }),
]),

// Bonus stacks on crit
OnCritHitRules(vec![
    Rule::new()
        .with_effect(Effect::Modify {
            stat: Stat::Stacks,
            op: Op::Add,
            value: 2.0,
        }),
]),

// Decrement countdown each frame, reset stacks when expired
OnTickRules(vec![
    // Decrement countdown
    Rule::new()
        .with_condition(Condition::Gt(Stat::Inactivity, 0.0))
        .with_effect(Effect::ModifyFrom {
            stat: Stat::Inactivity,
            op: Op::Subtract,
            from: Stat::DeltaTime,
        }),
    // Reset stacks when countdown expires
    Rule::new()
        .with_condition(Condition::Lte(Stat::Inactivity, 0.0))
        .with_condition(Condition::Gt(Stat::Stacks, 0.0))
        .with_effect(Effect::Set {
            stat: Stat::Stacks,
            value: 0.0,
        }),
]),
```

### Launcher Attack

```rust
OnPreHitRules(vec![
    Rule::new()
        .with_effect(Effect::Damage(Op::Set, 15.0))
        .with_effect(Effect::Knockback(Op::Set, 2.0))
        .with_effect(Effect::Launch(Op::Set, 10.0)),
]),
```

### Vacuum Pull

```rust
OnPreHitRules(vec![
    Rule::new()
        .with_effect(Effect::Damage(Op::Set, 5.0))
        .with_effect(Effect::Pull(Op::Set, 6.0)),
]),
```

### Execute (kill low HP enemies)

```rust
OnPreHitRules(vec![
    Rule::new()
        .with_condition(Condition::Lt(Stat::Health, 0.2))
        .with_effect(Effect::Damage(Op::Set, 9999.0)),
]),
```

## LLM Generation

The schema is finite and typed. An LLM can generate rules as JSON:

```json
{
  "conditions": [{ "ChanceFrom": "CritRate" }],
  "effects": [
    { "Set": { "stat": "HitIsCrit", "value": 1.0 } },
    { "Damage": ["Multiply", 2.5] },
    { "Knockback": ["Multiply", 2.5] }
  ]
}
```

Guidelines for LLM rule generation:
- Use **high-level effects** for damage and force (Damage, Knockback, Launch, etc.)
- Use **low-level** for other stats (Stacks, CritRate, HitIsCrit)
- Always specify the operation: `Op::Set`, `Op::Multiply`, `Op::Add`
- `OnPreHitRules` for damage/force computation
- `OnHitRules` for on-hit procs (stacks, lifesteal)
- `OnCritHitRules` fires automatically when `HitIsCrit` is set

## Adding New Behaviors

1. **New trigger type**: Add component in `triggers.rs`, add observer
2. **New condition**: Add variant to `Condition` enum, handle in `check_conditions`
3. **New stat**: Add variant to `Stat` enum
4. **New high-level effect**: Add variant to `Effect` enum (with `Op, f32`), handle in `execute_effects`

## Rule Presets (`src/rule_presets/`)

Presets are factory functions that return bundles of rule components for common patterns. They provide an even higher level of abstraction.

### Available Presets

**`rule_presets::crit(config)`** - Critical hit system
```rust
rule_presets::crit(CritConfig {
    crit_rate: 0.20,      // 20% chance
    damage_mult: 2.5,     // 2.5x damage on crit
    knockback_mult: 2.5,  // 2.5x knockback on crit
})
```

**`rule_presets::stacking(config)`** - Stacking attack speed buff
```rust
rule_presets::stacking(StackingConfig {
    gain_per_hit: 1.0,    // +1 stack per hit
    crit_bonus: 2.0,      // +2 bonus stacks on crit
    max_stacks: 12.0,     // Cap at 12 stacks
    decay_interval: 2.5,  // Reset stacks after 2.5s of inactivity
})
```

### Usage

```rust
commands.spawn((
    Player,
    rule_presets::crit(CritConfig::default()),
    rule_presets::stacking(StackingConfig::default()),
));
```

### Creating New Presets

Add a new file in `src/rule_presets/`:

```rust
// src/rule_presets/lifesteal.rs

pub struct LifestealConfig {
    pub percent: f32,
}

pub fn lifesteal(config: LifestealConfig) -> impl Bundle {
    OnHitRules(vec![
        Rule::new()
            .with_effect(Effect::Modify {
                stat: Stat::Health,
                op: Op::Add,
                value: /* damage * percent */,
            }),
    ])
}
```

## Design Principles

1. **Events are minimal** - Just source/target, no payload data
2. **Data flows through RuleVars** - Rules read/write stats, systems read final values
3. **High-level effects take (Op, value)** - Consistent API for set/multiply/add
4. **Derived triggers are automatic** - OnCritHit fires when HitIsCrit is set
5. **Geometry computed by systems** - Rules specify magnitudes, systems compute directions
6. **Presets compose rules** - Higher-level abstractions built on top of rules
