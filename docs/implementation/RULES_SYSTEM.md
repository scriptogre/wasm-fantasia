# Rules System

A compositional, data-driven system for game behaviors. Everything builds from small blocks into larger blocks.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│ LEVEL 0: Values                                                 │
│   f32, bool, Duration, Vec3, Entity                             │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│ LEVEL 1: Expressions (Expr)                                     │
│   Value(5.0), Stat(Health), Action(Damage),                     │
│   Add, Subtract, Multiply, Divide, Min, Max...                  │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│ LEVEL 2: Storage                                                │
│   Stats (persistent)   ←→  Stat enum   "who you are"            │
│   Action (per-action)  ←→  ActionVar enum   "what's happening"  │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│ LEVEL 3: Conditions (Condition)                                 │
│   GreaterThan, LessThan, Equals, Chance, All, Any, Not          │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│ LEVEL 4: Effects (Effect)                                       │
│   SetStat, SetAction, DealDamage, ApplyForce, ApplyBuff         │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│ LEVEL 5: Rules (Rule)                                           │
│   Rule { conditions, effects }                                  │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│ LEVEL 6: Triggers                                               │
│   OnPreHit, OnHit, OnTakeDamage, OnKill, OnTick                 │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│ LEVEL 7: Behaviors                                              │
│   Behavior { duration, stacks, triggers }                       │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│ LEVEL 8: Abilities                                              │
│   Ability { cost, cooldown, targeting, effects }                │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│ LEVEL 9: Entities                                               │
│   Entity { stats, abilities, behaviors }                        │
└─────────────────────────────────────────────────────────────────┘
```

---

## Level 0: Values

Primitive types that everything else builds on.

| Type | Description | Example |
|------|-------------|---------|
| `f32` | Numbers | `5.0`, `-3.5`, `0.25` |
| `bool` | True/false | `true`, `false` |
| `Duration` | Time spans | `2.5` seconds |
| `Vec3` | 3D vectors | Position, direction |
| `Entity` | ECS entity reference | Attacker, target |

---

## Level 1: Expressions (Expr)ju

Composable expressions that evaluate to a value. Used everywhere values are needed.

### Leaf Expressions

```rust
Value(f32)              // Literal number: Value(5.0)
Stat(Stat)              // Read from entity's persistent Stats: Stat(Health)
Action(ActionVar)       // Read from current action context: Action(Damage)
```

### Arithmetic

```rust
Add(Expr, Expr)         // a + b
Subtract(Expr, Expr)    // a - b
Multiply(Expr, Expr)    // a * b
Divide(Expr, Expr)      // a / b
Negate(Expr)            // -a
```

### Functions

```rust
Min(Expr, Expr)                         // min(a, b)
Max(Expr, Expr)                         // max(a, b)
Abs(Expr)                               // |a|
Floor(Expr)                             // floor(a)
Ceil(Expr)                              // ceil(a)
Clamp { value, min, max }               // clamp(value, min, max)
```

### Conditional

```rust
IfThenElse { cond, then, otherwise }    // if cond then a else b
```

### Examples (RON)

```ron
// Literal
Value(100.0)

// Read persistent stat
Stat(Health)

// Read action context
Action(Damage)

// AD * 2.0 + AP * 0.5
Add(
    Multiply(Stat(AttackDamage), Value(2.0)),
    Multiply(Stat(AbilityPower), Value(0.5)),
)

// clamp(Health, 0, MaxHealth)
Clamp(
    value: Stat(Health),
    min: Value(0.0),
    max: Stat(MaxHealth),
)

// if IsCrit > 0 then Damage * 2.5 else Damage
IfThenElse(
    cond: GreaterThan(Action(IsCrit), Value(0.0)),
    then: Multiply(Action(Damage), Value(2.5)),
    otherwise: Action(Damage),
)
```

---

## Level 2: Storage

Two separate storage systems for different lifetimes.

### Why Two Storages?

| Storage | Question it answers | Lifetime | Example |
|---------|---------------------|----------|---------|
| **Stats** | "Who are you?" | Persistent (lives with entity) | Health=100, AttackDamage=25 |
| **Action** | "What's happening now?" | Temporary (reset each action) | Damage=125, IsCrit=1 |

**Example: A critical hit**
```
Your AttackDamage stat is 25 (persistent, doesn't change)
                    ↓
            Attack happens
                    ↓
Action.Damage = 25 × 2.0 = 50 (base damage calculation)
                    ↓
            Crit triggers!
                    ↓
Action.Damage = 50 × 2.5 = 125 (crit multiplier)
                    ↓
            Armor reduces
                    ↓
Action.Damage = 100 (after armor)
                    ↓
        Target.Health -= 100

Your AttackDamage is still 25. Only Action.Damage was modified.
```

### Stats (Persistent) - "Who you are"

Entity stats that persist across actions. Stored as a Bevy component.

```rust
#[derive(Component)]
pub struct Stats(pub HashMap<Stat, f32>);

pub enum Stat {
    // === Core (engine reads/writes) ===
    Health,
    MaxHealth,

    // === Offensive ===
    AttackDamage,       // AD - physical scaling
    AbilityPower,       // AP - magic scaling

    // === Defensive ===
    Armor,              // Physical damage reduction
    MagicResist,        // Magic damage reduction

    // === Multipliers (engine reads) ===
    AttackSpeed,        // Animation speed multiplier
    MovementSpeed,      // Movement speed multiplier

    // === Combat ===
    CritChance,         // Crit probability (0.0-1.0)
    CritMultiplier,     // Crit damage multiplier

    // === Extension ===
    Custom(String),
}
```

### Action (Per-Action) - "What's happening now"

Temporary context for the current action. Created fresh, modified by rules, then consumed.

```rust
pub struct Action(pub HashMap<ActionVar, f32>);

pub enum ActionVar {
    // === Damage ===
    Damage,             // Computed damage for this hit
    DamageType,         // 0=physical, 1=magic, 2=true (convention)

    // === Force ===
    Knockback,          // Radial force (away from source)
    Launch,             // Vertical force (upward)
    Push,               // Forward force (source's facing)

    // === Flags ===
    IsCrit,             // 1.0 if critical hit

    // === Context ===
    Range,              // Attack range
    DeltaTime,          // Frame delta (for tick rules)

    // === Extension ===
    Custom(String),
}
```

### Data Flow

```
┌──────────────────┐                    ┌──────────────────┐
│      Stats       │                    │      Action      │
│   (persistent)   │                    │   (temporary)    │
├──────────────────┤                    ├──────────────────┤
│ Health: 100      │                    │ Damage: 125      │
│ AttackDamage: 25 │──── Expressions ──→│ Knockback: 5     │
│ AbilityPower: 10 │     read both      │ IsCrit: 1        │
│ CritChance: 0.2  │                    │                  │
│ CritMultiplier: 2.5                   │                  │
└──────────────────┘                    └──────────────────┘
         ↑                                       │
         │                                       ↓
         └──────────── Engine applies ───────────┘
                    Health -= Damage
```

---

## Level 3: Conditions (Condition)

Boolean expressions used in rules and conditional effects.

### Comparisons

```rust
GreaterThan(Expr, Expr)         // a > b
GreaterOrEqual(Expr, Expr)      // a >= b
LessThan(Expr, Expr)            // a < b
LessOrEqual(Expr, Expr)         // a <= b
Equals(Expr, Expr)              // a == b
```

### Random

```rust
Chance(Expr)        // random() < expr (probability check)
```

### Logical

```rust
All(Vec<Condition>)     // all conditions true (AND)
Any(Vec<Condition>)     // any condition true (OR)
Not(Condition)          // negate condition
```

### Examples (RON)

```ron
// Health below 50%
LessThan(Stat(Health), Multiply(Stat(MaxHealth), Value(0.5)))

// 20% chance
Chance(Value(0.2))

// Crit check using CritChance stat
Chance(Stat(CritChance))

// Low health AND has armor
All([
    LessThan(Stat(Health), Value(100.0)),
    GreaterThan(Stat(Armor), Value(0.0)),
])

// Either dead or out of mana
Any([
    LessOrEqual(Stat(Health), Value(0.0)),
    LessOrEqual(Stat(Mana), Value(0.0)),
])
```

---

## Level 4: Effects (Effect)

Atomic actions that change state.

### Variable Modification

```rust
SetStat { stat: Stat, value: Expr }         // Set persistent stat
SetAction { var: ActionVar, value: Expr }   // Set action context variable
```

### Combat Actions

```rust
DealDamage                              // Apply Action(Damage) to target's Health
ApplyForce                              // Apply Knockback/Launch/Push to target
```

### Entity Actions

```rust
SpawnEntity { template: String }        // Create entity from template
DestroyEntity                           // Remove entity
```

### Visual/Audio

```rust
SpawnVfx { asset: String }              // Play visual effect
PlaySound { asset: String }             // Play sound effect
```

### Buff Management

```rust
ApplyBuff { behavior: String, stacks: u32 }
RemoveBuff { behavior: String }
ModifyStacks { behavior: String, amount: i32 }
```

### Debug

```rust
Log(String)                             // Print debug message
```

### Examples (RON)

```ron
// Set damage to AD * 2
SetAction(var: Damage, value: Multiply(Stat(AttackDamage), Value(2.0)))

// Heal 10% of max health
SetStat(stat: Health, value: Add(
    Stat(Health),
    Multiply(Stat(MaxHealth), Value(0.1)),
))

// Double knockback
SetAction(var: Knockback, value: Multiply(Action(Knockback), Value(2.0)))

// Mark as critical hit
SetAction(var: IsCrit, value: Value(1.0))
```

---

## Level 5: Rules (Rule)

A condition-effect pair. If all conditions pass, effects execute in order.

```rust
pub struct Rule {
    pub conditions: Vec<Condition>,  // All must be true (implicit AND)
    pub effects: Vec<Effect>,        // Execute in order
}
```

### Examples (RON)

```ron
// Critical hit rule
(
    conditions: [Chance(Stat(CritChance))],
    effects: [
        SetAction(var: IsCrit, value: Value(1.0)),
        SetAction(var: Damage, value: Multiply(Action(Damage), Stat(CritMultiplier))),
    ],
)

// Execute low health enemies (< 20% health)
(
    conditions: [LessThan(Stat(Health), Multiply(Stat(MaxHealth), Value(0.2)))],
    effects: [
        SetAction(var: Damage, value: Value(9999.0)),
    ],
)

// Lifesteal on hit (15%)
(
    conditions: [],  // Always triggers
    effects: [
        SetStat(stat: Health, value: Add(
            Stat(Health),
            Multiply(Action(Damage), Value(0.15)),
        )),
    ],
)

// Armor reduces physical damage
(
    conditions: [Equals(Action(DamageType), Value(0.0))],  // Physical only
    effects: [
        SetAction(var: Damage, value: Multiply(
            Action(Damage),
            Divide(Value(100.0), Add(Value(100.0), Stat(Armor))),
        )),
    ],
)
```

---

## Level 6: Triggers

Events that cause rules to evaluate.

| Trigger | When | Context Available |
|---------|------|-------------------|
| `OnPreHit` | Before hit resolves | Attacker stats, Ctx being computed |
| `OnHit` | After hit lands | Attacker stats, final Ctx |
| `OnCritHit` | After critical hit | Same as OnHit |
| `OnKill` | After killing target | Killer stats |
| `OnTakeDamage` | When receiving damage | Defender stats, incoming Ctx |
| `OnDeath` | When entity dies | Dying entity stats |
| `OnTick` | Every frame | DeltaTime in Ctx |
| `OnTimer(f32)` | Periodically | Interval in seconds |

### Trigger Components

```rust
#[derive(Component)]
pub struct OnPreHitRules(pub Vec<Rule>);

#[derive(Component)]
pub struct OnHitRules(pub Vec<Rule>);

// ... etc
```

### Event Flow Example

```
Attack initiated
       ↓
ActionCtx created with base values
       ↓
OnPreHitRules execute (attacker)
    - Crit check
    - Damage scaling
    - Special effects
       ↓
OnTakeDamageRules execute (defender)
    - Armor reduction
    - Shields
       ↓
Engine applies final Damage to Health
       ↓
OnHitRules execute (attacker)
    - Lifesteal
    - Stack gain
       ↓
If target died: OnKillRules execute
```

---

## Level 7: Behaviors

Attachable rule containers with duration and stacking.

```rust
pub struct Behavior {
    pub id: String,
    pub duration: Option<f32>,      // None = permanent
    pub max_stacks: u32,
    pub stack_behavior: StackBehavior,
    pub triggers: BehaviorTriggers,
    pub stat_modifiers: Vec<StatModifier>,
}

pub enum StackBehavior {
    Refresh,        // Reset duration on reapply
    Stack,          // Add stack, keep duration
    Extend,         // Add duration
    None,           // Ignore reapply
}

pub struct BehaviorTriggers {
    pub on_apply: Vec<Effect>,
    pub on_remove: Vec<Effect>,
    pub on_tick: Vec<Rule>,
    pub on_hit: Vec<Rule>,
    // ... other triggers
}

pub struct StatModifier {
    pub stat: Stat,
    pub op: ModOp,          // Add, Multiply, Set
    pub value: Expr,
    pub per_stack: bool,
}
```

### Example: Burning Debuff

```ron
(
    id: "burning",
    duration: Some(3.0),
    max_stacks: 5,
    stack_behavior: Stack,
    triggers: (
        on_tick: [(
            conditions: [],
            effects: [
                // Deal 10 damage per stack per second
                SetStat(stat: Health, value: Subtract(
                    Stat(Health),
                    Multiply(Value(10.0), Multiply(Stat(Custom("BurningStacks")), Action(DeltaTime))),
                )),
            ],
        )],
    ),
)
```

### Example: Attack Speed Buff

```ron
(
    id: "attack_speed_buff",
    duration: Some(5.0),
    max_stacks: 1,
    stack_behavior: Refresh,
    stat_modifiers: [
        (stat: AttackSpeed, op: Multiply, value: Value(1.5), per_stack: false),
    ],
)
```

---

## Level 8: Abilities

Activatable actions with cost, cooldown, and targeting.

```rust
pub struct Ability {
    pub id: String,
    pub cost: AbilityCost,
    pub cooldown: f32,
    pub targeting: Targeting,
    pub effects: Vec<Effect>,
    pub on_hit_rules: Vec<Rule>,
}

pub enum AbilityCost {
    None,
    Mana(Expr),
    Health(Expr),
    Custom { resource: Stat, amount: Expr },
}

pub enum Targeting {
    SelfTarget,
    SingleEnemy { range: Expr },
    SingleAlly { range: Expr },
    PointTarget { range: Expr },
    Cone { range: Expr, angle: Expr },
    Circle { range: Expr, radius: Expr },
    Projectile { range: Expr, speed: Expr },
}
```

### Example: Fireball

```ron
(
    id: "fireball",
    cost: Mana(Value(50.0)),
    cooldown: 5.0,
    targeting: Projectile(range: Value(10.0), speed: Value(15.0)),
    effects: [
        // 80 base + 60% AP scaling
        SetAction(var: Damage, value: Add(Value(80.0), Multiply(Stat(AbilityPower), Value(0.6)))),
        SetAction(var: DamageType, value: Value(1.0)),  // Magic damage
        ApplyBuff(behavior: "burning", stacks: 1),
    ],
)
```

### Example: Sword Slash

```ron
(
    id: "sword_slash",
    cost: None,
    cooldown: 0.5,
    targeting: Cone(range: Value(3.0), angle: Value(120.0)),
    effects: [
        // 10 base + 100% AD scaling
        SetAction(var: Damage, value: Add(Value(10.0), Stat(AttackDamage))),
        SetAction(var: DamageType, value: Value(0.0)),  // Physical damage
        SetAction(var: Knockback, value: Value(2.0)),
    ],
    on_hit_rules: [(
        conditions: [Chance(Stat(CritChance))],
        effects: [
            SetAction(var: IsCrit, value: Value(1.0)),
            SetAction(var: Damage, value: Multiply(Action(Damage), Stat(CritMultiplier))),
        ],
    )],
)
```

---

## Level 9: Entities

Compositions of stats, abilities, and behaviors.

```rust
pub struct EntityTemplate {
    pub id: String,
    pub stats: HashMap<Stat, f32>,
    pub abilities: Vec<String>,
    pub default_behaviors: Vec<String>,
}
```

### Example: Player

```ron
(
    id: "player",
    stats: {
        Health: 100.0,
        MaxHealth: 100.0,
        AttackDamage: 25.0,
        AbilityPower: 10.0,
        Armor: 5.0,
        MagicResist: 5.0,
        AttackSpeed: 1.0,
        MovementSpeed: 1.0,
        CritChance: 0.1,
        CritMultiplier: 2.0,
    },
    abilities: ["sword_slash", "fireball", "dash"],
    default_behaviors: [],
)
```

### Example: Goblin

```ron
(
    id: "goblin",
    stats: {
        Health: 50.0,
        MaxHealth: 50.0,
        AttackDamage: 10.0,
        Armor: 2.0,
        AttackSpeed: 1.2,
        MovementSpeed: 0.8,
    },
    abilities: ["goblin_attack"],
    default_behaviors: [],
)
```

---

## Composition Examples

### Critical Hit System

Built from primitives:

```ron
// As OnPreHitRules on attacker
(
    conditions: [Chance(Stat(CritChance))],
    effects: [
        SetAction(var: IsCrit, value: Value(1.0)),
        SetAction(var: Damage, value: Multiply(Action(Damage), Stat(CritMultiplier))),
        SetAction(var: Knockback, value: Multiply(Action(Knockback), Value(2.0))),
    ],
)
```

### Armor System

Built from primitives:

```ron
// As OnTakeDamageRules on defender
// Physical damage reduction: Damage = Damage * (100 / (100 + Armor))
(
    conditions: [Equals(Action(DamageType), Value(0.0))],  // Physical only
    effects: [
        SetAction(var: Damage, value: Multiply(
            Action(Damage),
            Divide(Value(100.0), Add(Value(100.0), Stat(Armor))),
        )),
    ],
)
```

### Stacking Attack Speed Buff

Built from primitives:

```ron
// Behavior definition
(
    id: "frenzy",
    duration: None,  // Permanent until decay
    max_stacks: 12,
    stack_behavior: Stack,
    stat_modifiers: [
        // +12% attack speed per stack
        (stat: AttackSpeed, op: Add, value: Value(0.12), per_stack: true),
    ],
    triggers: (
        on_tick: [
            // Decrement decay timer
            (
                conditions: [GreaterThan(Stat(Custom("FrenzyDecay")), Value(0.0))],
                effects: [SetStat(
                    stat: Custom("FrenzyDecay"),
                    value: Subtract(Stat(Custom("FrenzyDecay")), Action(DeltaTime)),
                )],
            ),
            // Remove buff when timer expires
            (
                conditions: [LessOrEqual(Stat(Custom("FrenzyDecay")), Value(0.0))],
                effects: [RemoveBuff(behavior: "frenzy")],
            ),
        ],
    ),
)

// OnHitRules to apply/refresh the buff
(
    conditions: [],
    effects: [
        ApplyBuff(behavior: "frenzy", stacks: 1),
        SetStat(stat: Custom("FrenzyDecay"), value: Value(2.5)),
    ],
)
```

---

## Design Principles

1. **Everything composes** - Small blocks build into larger blocks
2. **Two storages with clear purpose**
   - `Stats` = "who you are" (persistent entity data)
   - `Action` = "what's happening now" (temporary per-action data)
3. **Expressions everywhere** - Values are computed, never hardcoded
4. **Rules are data** - No code required for new behaviors
5. **Engine interface is minimal** - Engine only reads final `Damage`, `Knockback`, `Health`
6. **Extension via Custom** - Unknown future needs use `Custom(String)`

---

## Bevy Integration

This system integrates with Bevy's ECS:

| Concept | Bevy Implementation |
|---------|---------------------|
| `Stats` | `Component` on entities |
| `Action` | Created per-action, passed through rule evaluation |
| Triggers | Bevy `Observer`s on events (`HitEvent`, `DamageEvent`, etc.) |
| Behaviors | `Component` with timer, removed when expired |
| Abilities | `Component` or `Resource` defining available actions |

The rules system is an **additional layer** on top of ECS, not a replacement. Bevy handles entity management, events, and system scheduling. The rules system handles data-driven behavior evaluation.
