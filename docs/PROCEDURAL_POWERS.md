# Procedural Powers

Generate contextual "god powers" per session using LLMs as **System Configurators**, not asset creators.

---

## Core Principle

The LLM doesn't create assets. It remixes existing flexible systems by outputting structured configuration.

| Wrong Approach               | Right Approach                                                      |
|------------------------------|---------------------------------------------------------------------|
| "Generate a fireball sprite" | "Configure `generic_projectile` with `color: orange, trail: ember`" |
| "Create a new sword mesh"    | "Set `vertex_displacement: spikes, intensity: 2.0`"                 |
| "Design a unique ability"    | "Chain `on_kill → spawn_projectile → split(3)`"                     |

---

## Universal Schema

One schema handles god powers, items, consumables, and curses. The trigger determines behavior.

```python
class PowerUp(BaseModel):
    name: str                       # "Vampiric Loan"
    rarity: Rarity                  # Common | Legendary | Cursed
    tags: list[str]                 # ["melee", "blood_magic", "high_risk"]
    
    # State Machine Memory
    variables: dict                 # Local storage (e.g., {"blood_debt": 0})
    
    # Core mechanics
    logic_chain: list[LogicBlock]
    
    # Risk/reward trade-offs
    constraints: list[Constraint]
    
    # Visuals & Lore
    visuals: VFXConfig | None
    flavor_text: str                # "Interest is collected in blood."
```

---

## Mechanic System: Logic Legos

Expose composable logic blocks. Powers act as mini State Machines with memory, spatial awareness, and flow control.

**Triggers**: `on_hit`, `on_kill`, `on_dodge`, `every_x_seconds`, `on_take_damage`, `on_activate`, `on_tick`, `on_consume`

**Payloads**: `spawn_projectile`, `explode`, `apply_status`, `teleport`, `pull_enemies`, `heal`, `modify_variable`, `self_damage`, `query_area`, `spawn_entity`, `create_zone_line`

**Flow Control**: `check_variable`, `if_else`, `distance_to`, `loop_x_times`, `entity_exists`

**Modifiers**: `chain`, `pierce`, `split`, `grow`

**Example**: "The Spectral Clothesline" (Spatial Choreography)

```json
{
  "name": "Spectral Clothesline",
  "tags": ["positioning", "high_skill"],
  "variables": { "anchor_id": null },
  "logic_chain": [
    {
      "trigger": "on_activate",
      "action": "spawn_entity",
      "params": { "type": "anchor_unit", "duration": 10.0 }
    },
    {
      "trigger": "on_tick",
      "conditions": ["entity_exists(anchor_id)"],
      "action": "create_zone_line",
      "params": {
        "start": "player.position",
        "end": "anchor.position",
        "effect": "damage_per_second"
      }
    }
  ]
}

```

Result: Movement-based geometry weapon. Connecting the player to an anchor with a damaging beam.

---

## Synergy System

LLM sees the full player build (as tags) to generate compounding interactions.

**Context Injection:**

```json
{
  "biome": "volcanic",
  "player_state": {
    "tags": ["crit_high", "fast_attacks", "fire_element"],
    "weakness": "low_hp"
  }
}

```

**Synergy Output:**

```json
{
  "name": "Ignition Spark",
  "tags": ["fire", "on_hit", "attack_speed_scaling"],
  "logic_chain": [
    {
      "trigger": "every_nth_hit",
      "params": { "n": 5 },
      "action": "explode",
      "action_params": { 
        "element": "fire", 
        "scaling": "attack_speed * 2.0" 
      }
    }
  ]
}

```

Result: Build identity compounds. The LLM bridges "Fast Attacks" with "Fire" to create a high-frequency explosion loop.

---

## Visual System: VFX Synthesizer

Build ~20 flexible base effects. Expose their parameters to the LLM.

**Base Assets**: `generic_beam`, `generic_explosion`, `generic_projectile`, `generic_aura`, `generic_orb`

**LLM Control Board**:

```python
class VFXConfig:
    base: str           # "generic_orb"
    color: str          # "#800080"
    count: int          # 12
    scale: float        # 1.5
    speed: float        # 2.0
    behavior: Behavior  # orbit | seek | pierce | bounce
    trail_intensity: float
    shader_noise: float # Distortion amount

```

**Example**: LLM receives "void-themed god power" context.

```json
{
  "base": "generic_orb",
  "color": "#800080",
  "count": 12,
  "trail_intensity": 0.9,
  "shader_noise": 0.8
}

```

Result: 12 jagged purple orbs with heavy trails. No new assets.

---

## Weapon Evolution

No runtime modeling. Use shaders and attachment points.

**Shader Displacement**:

```json
{
  "displacement_type": "spikes",
  "intensity": 2.0
}

```

Smooth sword → jagged crystalline spikes.

**Hardpoint Attachments**:

```json
{
  "socket_tip": "lightning_arc",
  "socket_hilt": "void_drip"
}

```

Complex visual silhouette, same base mesh.

---

## Boundaries (Schema Constraints)

Use constrained decoding (`instructor`, `outlines`) to prevent hallucination.

```python
class Trigger(Enum):
    ON_HIT = "on_hit"
    ON_KILL = "on_kill"
    ON_DODGE = "on_dodge"
    EVERY_X_SECONDS = "every_x_seconds"
    ON_TAKE_DAMAGE = "on_take_damage"
    ON_CONSUME = "on_consume"

class Action(Enum):
    EXPLODE = "explode"
    SPAWN_PROJECTILE = "spawn_projectile"
    APPLY_STATUS = "apply_status"
    TELEPORT = "teleport"
    PULL_ENEMIES = "pull_enemies"
    HEAL = "heal"
    MODIFY_VARIABLE = "modify_variable"
    SELF_DAMAGE = "self_damage"

class LogicBlock(BaseModel):
    trigger: Trigger
    action: Action
    params: dict
    visuals: VFXConfig | None

```

LLM literally cannot output anything outside the schema.

---

## Generation Flow

```
Player enters session
        ↓
Context assembled: { biome, threat_level, player_history, world_state, player_tags }
        ↓
LLM generates 3-5 powers (structured output, <500ms)
        ↓
Schema validation (damage caps, cooldown minimums)
        ↓
Engine instantiates: attaches listeners, sets VFX params, initializes variables
        ↓
Player sees "bespoke" power (actually just configured knobs)

```

---

## Evolution During Session

Powers track performance metrics. When thresholds hit, LLM evolves them.

```json
{
  "current_power": { "name": "Molten Dominion", "..." },
  "session_stats": { "kills": 1200, "aoe_ratio": 0.85 },
  "constraint": "must_amplify_aoe_fantasy"
}

```

Output: Evolved power with new mechanic chain + adjusted visuals.

---

## Design Alignment

| Principle | Implementation |
| --- | --- |
| Autonomy | Powers are deterministic from context, not gacha |
| Competence | Evolution unlocks are visible achievements |
| Significance | `market_synergy` field ties powers to world economy |
| Synergy | Tags compound across session to enable "broken" builds |
| Complexity | State Machine logic enables interaction over simple stats |

```