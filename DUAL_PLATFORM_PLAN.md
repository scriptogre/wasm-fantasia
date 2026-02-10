# Dual-Platform Strategy: SpacetimeDB Everywhere

## Context

Currently, singleplayer and multiplayer are two completely different code paths. Singleplayer runs game logic (enemy AI, combat, spawning) client-side in Bevy systems. Multiplayer runs the same logic server-side in SpacetimeDB reducers. This means game logic is duplicated in two places — and they drift.

**Goal**: Every game session connects to SpacetimeDB. Native singleplayer launches a local SpacetimeDB subprocess. Web always connects to a remote server. Multiplayer connects to a remote server on both platforms. Solo web play creates a private remote session. One code path, one source of truth.

**Decisions**:
- Native singleplayer always requires the embedded SpacetimeDB subprocess (no offline fallback)
- Web shows both "Solo" (private remote session) and "Multiplayer" buttons
- Building against current SpacetimeDB 1.12 API

---

## Implementation Status

| Phase | Status | Summary |
|-------|--------|---------|
| 0 — Semantic Refactoring | **Done** | `ServerTarget` resource, `is_server_connected` run condition |
| 1 — Local Subprocess Manager | **Done** | `networking/local_server.rs` — start, health-check, deploy, shutdown |
| 2 — SP Connects to Local DB | **Done** | Title screen flows, connecting screen, generalized run conditions |
| 3 — Remove Client-Side SP Logic | **Done** | Deleted `enemy_ai`, `EnemyAi`, SP spawn fallback, lag simulator |
| 4 — Web Solo Sessions | **Done** | `world_id` isolation on all tables, scoped AI/spawning/subscriptions |
| 5 — Distribution & Polish | **Done** | Binary bundling, pre-compiled WASM deploy, server prewarm |

### Verified
- Native singleplayer (local SpacetimeDB subprocess) — tested, working
- Native multiplayer (remote SpacetimeDB) — tested, working
- SP → MP → SP transitions — no stale URI leaks
- Resume / New Game from title screen — server persists, instant reconnect
- Web compilation (`just check` + `just web`) — passes
- `cargo check --features multiplayer` — zero warnings

### Additional cleanup (not in original plan)
- Removed `LagSimulator`, `LagBuffers`, `PendingOutboundUpdate`, `process_outbound_lag`
- Inlined `attack_animation()` helper into `send_local_position`
- Renamed `is_remote` → `is_server_owned` in damage.rs for clarity
- cfg-gated WASM-incompatible methods in generated SpacetimeDB bindings
- Added `just generate` recipe for safe binding regeneration
- Composable button API (`btn`/`btn_disabled` replace `btn_big`/`btn_small`/`btn_tiny`)
- Fixed game rendering/unpausing during Gameplay → Title transition

---

## Phase 0: Semantic Refactoring (No Behavior Change) — DONE

**Goal**: Prepare the type system for the new model without changing any runtime behavior.

### 0A: Add `ServerTarget` resource

`client/src/models/states.rs` — Added alongside existing `GameMode`:

```rust
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub enum ServerTarget {
    Local { port: u16 },
    Remote { uri: String },
}
```

`GameMode` keeps its current meaning (Singleplayer vs Multiplayer = social mode). `ServerTarget` describes where the SpacetimeDB instance lives.

### 0B: Add `is_server_connected` run condition

`client/src/networking/mod.rs`:

```rust
pub fn is_server_connected(conn: Option<Res<SpacetimeDbConnection>>) -> bool {
    conn.is_some()
}
```

**Files changed**: `client/src/models/states.rs`, `client/src/networking/mod.rs`

---

## Phase 1: Local SpacetimeDB Subprocess Manager — DONE

**Goal**: A standalone module that can start, health-check, deploy to, and shut down a local SpacetimeDB instance. Native only.

### Implementation: `client/src/networking/local_server.rs`

Resources:
- `LocalServer` — handle to subprocess, port, binary path
- `LocalServerState` — Starting → WaitingForReady → Deploying → Ready / Failed

Key operations:
1. **Binary discovery**: Adjacent to executable → `SPACETIMEDB_PATH` env → `~/.local/bin/spacetime` → system PATH
2. **Port selection**: `TcpListener::bind("127.0.0.1:0")` for random available port
3. **Start**: `spacetime start --listen-addr 127.0.0.1:<port> --in-memory --data-dir <temp>`
4. **Health check**: Port probe — if `TcpListener::bind` fails, port is occupied = server listening
5. **Deploy**: `spacetime publish wasm-fantasia --bin-path <wasm>` (pre-compiled) or `--project-path server` (dev)
6. **Shutdown**: `child.kill()` + `child.wait()`, also in `Drop` impl

Gated with `#[cfg(not(target_arch = "wasm32"))]`. Registered as plugin inside `NetworkingPlugin`.

**Files**: New `client/src/networking/local_server.rs`, `client/Cargo.toml` (added `home = "0.5"`)

---

## Phase 2: Singleplayer Connects to Local SpacetimeDB — DONE

**Goal**: Clicking "Singleplayer" on native starts the local server, deploys, connects, and enters gameplay.

### Title screen changes

**Native**: "Singleplayer" → `GameMode::Singleplayer` + `ServerTarget::Local` → starts local server → `Screen::Connecting`
**Native (resume)**: "Resume" / "New Game" split buttons when server already running
**WASM**: "Solo" → `GameMode::Singleplayer` + `ServerTarget::Remote` → `Screen::Connecting`
**Both**: "Multiplayer" → `GameMode::Multiplayer` + `ServerTarget::Remote` → `Screen::Connecting`

### Connecting screen

Handles both local server startup and remote connections:
- `advance_local_server` system drives `LocalServerState` (native only)
- Detects already-running server and skips straight to connecting
- `spawn_connecting_screen` reads `ServerTarget` to set URI and initial log message
- Systems chained: advance_local_server → track_connection_state → tick_connection → tick_timeout → update_log_display

### Networking run conditions generalized

- `OnEnter(Screen::Connecting)` reset timer: `resource_exists::<ServerTarget>` (was `is_multiplayer_mode`)
- `OnExit(Screen::Connecting)` disconnect: `resource_exists::<ServerTarget>` (was `is_multiplayer_mode`)
- `OnExit(Screen::Gameplay)` disconnect + remove target: `is_server_connected` (was `is_multiplayer_mode`)
- `auto_connect`: checks `ServerTarget` existence (was `GameMode::Multiplayer`); waits for `LocalServerState::Ready` on native

### Cleanup on exit

- SP with running server: connection and server preserved for resume
- MP or failed server: full cleanup (disconnect, remove resources)
- `to::title` removes `ServerTarget`; session reset deferred to `OnEnter(Title)`

**Files changed**: `client/src/screens/title.rs`, `client/src/screens/mod.rs`, `client/src/screens/connecting.rs`, `client/src/networking/mod.rs`

---

## Phase 3: Remove Client-Side Singleplayer Logic — DONE

**Goal**: Delete the duplicated client-side game logic. Server module is the single source of truth.

### Removed
- `enemy_ai` system (~130 lines including spatial hash separation)
- `EnemyAi` component and its `Default` impl
- `SharedEnemyAssets` resource and `setup_shared_enemy_assets` system
- Offline spawn fallback in `spawn_enemy_in_front` (local enemy spawning loop)
- `is_paused` run condition on enemy AI

### Simplified
- `spawn_enemy_in_front`: only calls server reducer, warns if no connection
- `on_enemy_added`: builds animation graph per-entity (no shared assets resource)
- `damage.rs`: renamed `is_remote` → `is_server_owned`, improved doc comments

**Files changed**: `client/src/combat/enemy.rs`, `client/src/combat/components.rs`, `client/src/combat/damage.rs`

---

## Phase 4: Web Solo Sessions (Server-Side) — DONE

**Goal**: Support "Solo" mode on web where the player gets a private session on the remote server.

### 4A: Server-side world isolation

`server/src/lib.rs`:
- `world_id` field on `Player`, `Enemy`, and `CombatEvent` tables
- `join_game` reducer accepts `world_id` parameter
- Solo players use their identity hex as world_id; multiplayer uses `"shared"`
- Enemy spawning scoped to world: enemies inherit spawner's `world_id`
- `game_tick` groups players and enemies by `world_id`, AI only chases within same world
- Solo disconnect cleans up all enemies and combat events for that world

### 4B: Client sends world_id

`client/src/networking/mod.rs`:
- Connection builder computes `world_id` from `GameMode` and identity
- Subscription queries filter all tables by `world_id` via WHERE clauses

**Files changed**: `server/src/lib.rs`, `client/src/networking/mod.rs`

---

## Phase 5: Distribution & Polish — DONE

**Goal**: Native release builds are self-contained.

### 5A: Bundle SpacetimeDB binary

- `find_spacetime_binary()` checks adjacent to game executable first, then env/PATH fallbacks
- Works for macOS, Linux, Windows bundled distributions

### 5B: Pre-compile server module

- `spawn_deploy()` checks for `wasm_fantasia_module.wasm` adjacent to executable
- Uses `--bin-path` if found, falls back to `--project-path server` for dev workflow
- Eliminates Rust toolchain requirement on player machines

### 5C: Justfile targets

- `just bundle-native` — builds server WASM module + native client + copies spacetime binary + assets to `dist/native/`

### 5D: Startup time optimization

- Local server prewarmed during loading screen (`OnEnter(Screen::Loading)`)
- Server persists across SP sessions — resume reconnects instantly
- New Game kills old server and starts fresh

**Files changed**: `Justfile`, `client/src/networking/local_server.rs`

---

## Dependency Graph

```
Phase 0 (Refactoring) ─── done
    │
Phase 1 (Subprocess Manager) ─── done
    │
Phase 2 (SP Uses Local DB) ─── done
    │
    ├── Phase 3 (Delete Client-Side SP Logic) ─── done
    │
    ├── Phase 4 (Web Solo Sessions) ─── done
    │
    └── Phase 5 (Distribution) ─── done
```
