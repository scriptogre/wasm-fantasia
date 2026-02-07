# Networking Architecture

SpacetimeDB + Bevy ECS multiplayer model. **The server is always authoritative.** Clients predict locally for responsiveness, but the server's state is the truth. Every piece of game state falls into one of three categories.

## State Ownership

### Server-Authoritative

All table state is owned by the server. Clients read from table subscriptions and never modify table-synced values locally.

| Table          | Fields                                             | Modified By                       |
|----------------|----------------------------------------------------|-----------------------------------|
| `player`       | `health`, `max_health`                             | `attack_connect`, `respawn`       |
| `player`       | `stacks`, `attack_speed`, `stack_decay`            | `attack_connect` (stacking logic) |
| `player`       | `last_attack_time`, `last_hit_time`                | `attack_connect`                  |
| `player`       | `x`, `y`, `z`, `rot_y`                            | `update_position` reducer         |
| `player`       | `anim_state`, `attack_seq`, `attack_anim`          | `update_position` reducer         |
| `npc_enemy`    | All fields (`id`, `x/y/z`, `health`, `max_health`) | `spawn_enemies`, `attack_connect` |
| `combat_event` | All fields (ephemeral hit results)                 | `attack_connect`                  |

### Client-Side Prediction (local simulation, server reconciliation)

For responsiveness, clients run simulation locally and send requests to the server. The server validates and writes the authoritative state. Clients reconcile when the server state arrives.

| What's Predicted | How Client Handles It                                                                                |
|------------------|------------------------------------------------------------------------------------------------------|
| Movement         | Client runs Tnua physics locally, sends position to server via `update_position`. Server stores it.  |
| Attack animation | Client starts animation immediately on input. Sends `attack_seq`/`attack_anim` to server.           |
| Hit detection    | Client fires `attack_connect()` reducer. Server validates range/arc/cooldown and resolves damage.    |

**Current state:** The `update_position` reducer is a relay — it stores what the client sends. This is the simplest form of prediction (no server validation of movement). Future work could add server-side movement validation or anti-cheat checks.

**Future work:** Full client-side prediction with server reconciliation (rollback on mismatch). For now, movement is predicted-and-trusted, combat is server-validated.

### Client-Local (never networked)

Pure ECS components. No table, no reducer, no sync.

- Camera orbit, zoom, position
- Screen shake, hit stop (time freeze)
- VFX particles, impact effects
- Damage number entities
- Health bar UI
- Hit flash timers
- Sound effects
- Input buffers, attack timing (`AttackState`)
- Separation forces
- Debug hitboxes, diagnostics

## Combat Flow

```
Local Client                    Server                     Remote Client
─────────────                   ──────                     ─────────────
1. Player presses attack
2. Start attack animation
   immediately (predicted)
3. Send attack_seq++, attack_anim
   via update_position reducer
                                4. Store attack_seq/attack_anim
                                   in player table
                                   (broadcast to all subscribers)
5. At hit_time, call
   attack_connect() reducer
                                6. Validate hit (range, arc,
                                   cooldown, position)
                                7. Roll crit, compute damage
                                8. Update target health in
                                   player/npc_enemy table
                                9. Insert combat_event row
                                   for each target hit
                               10. Update attacker stacks/speed

11. Receive combat_event                                   11. Receive combat_event
    via on_insert callback                                     via on_insert callback
12. Trigger HitEvent on target                             12. Trigger HitEvent on target
    → damage numbers, flash, VFX                               → damage numbers, flash, VFX
13. Receive health update                                  13. Receive health update
    via player table subscription                              via player table subscription
                                                           14. Receive attack_seq change
                                                               → play attack_anim on remote
```

## Tables

### `player` (existing, extended)

All fields are server-authoritative. Some are written by reducers the owning client calls (position, animation), others by game logic reducers (health, combat stats).

```
identity        PK          -- SpacetimeDB identity
name            String?     -- Display name
online          bool        -- Connection status

-- Written by update_position reducer (client requests, server stores)
x, y, z         f32         -- World position
rot_y           f32         -- Y-axis rotation (yaw)
anim_state      String      -- Movement animation key
attack_seq      u32         -- Monotonic attack counter
attack_anim     String      -- Attack animation clip name

-- Written by game logic reducers (attack_connect, respawn, etc.)
health          f32
max_health      f32
attack_damage   f32
crit_chance     f32
crit_multiplier f32
attack_range    f32
attack_arc      f32
knockback_force f32
attack_speed    f32
stacks          f32
stack_decay     f32
last_hit_time   i64
last_attack_time i64
last_update     i64
```

### `combat_event` (new, ephemeral)

```
id              PK auto_inc
attacker        Identity    -- Who dealt the hit
target_player   Identity?   -- Hit player (if any)
target_npc_id   u64?        -- Hit NPC id (if any)
damage          f32         -- Final damage dealt
is_crit         bool        -- Was this a critical hit
attacker_x      f32         -- Attacker position at hit time (for knockback direction)
attacker_z      f32
timestamp       i64         -- Server timestamp
```

Cleanup: `attack_connect` deletes events older than 5 seconds at the start of each call.

### `npc_enemy` (existing, unchanged)

Fully server-authoritative. Clients read position and health from table subscriptions.

## Client-Side Processing Rules

1. **Never call `health.take_damage()` on entities with `RemotePlayer` or `ServerEnemy`** — health comes from the server via table sync.
2. **Never despawn `RemotePlayer` or `ServerEnemy`** — server owns lifecycle.
3. **VFX triggers come from `combat_event` inserts**, not local hit detection.
4. **Attack animations are predicted** — start immediately on input, don't wait for server.
5. **Remote attack animations are driven by `attack_seq`/`attack_anim`** from the player table.
6. **Movement is predicted** — client runs physics locally, sends result to server. Server stores and broadcasts.

## Subscription Queries

```sql
SELECT * FROM player
SELECT * FROM npc_enemy
SELECT * FROM combat_event
```
