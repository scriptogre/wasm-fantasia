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
| 0 — Semantic Refactoring | **Code written** | `ServerTarget` resource, `is_server_connected` run condition |
| 1 — Local Subprocess Manager | **Code written** | `networking/local_server.rs` — start, health-check, deploy, shutdown |
| 2 — SP Connects to Local DB | **Code written** | Title screen flows, connecting screen, generalized run conditions |
| 3 — Remove Client-Side SP Logic | **Code written** | Deleted `enemy_ai`, `EnemyAi`, SP spawn fallback, lag simulator |
| 4 — Web Solo Sessions | **Not started** | Server-side session isolation needed |
| 5 — Distribution & Polish | **Not started** | Binary bundling, pre-compiled WASM module |

### Known issues
- **Local server crashes on startup** — `spacetime start` exits immediately with status 1. Needs debugging (likely missing args, wrong binary path, or SpacetimeDB version incompatibility). Native singleplayer is non-functional until this is fixed.
- **Compilation not verified** — `cargo check` and `just check` have not been run against these changes yet

### Additional cleanup (not in original plan)
- Removed `LagSimulator`, `LagBuffers`, `PendingOutboundUpdate`, `process_outbound_lag`
- Inlined `attack_animation()` helper into `send_local_position`
- Renamed `is_remote` → `is_server_owned` in damage.rs for clarity

---

## Phase 0: Semantic Refactoring (No Behavior Change) — CODE WRITTEN

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

## Phase 1: Local SpacetimeDB Subprocess Manager — CODE WRITTEN (local server crashes at runtime)

**Goal**: A standalone module that can start, health-check, deploy to, and shut down a local SpacetimeDB instance. Native only.

### Implementation: `client/src/networking/local_server.rs`

Resources:
- `LocalServer` — handle to subprocess, port, binary path
- `LocalServerState` — Starting → WaitingForReady → Deploying → Ready / Failed

Key operations:
1. **Binary discovery**: `SPACETIMEDB_PATH` env → `~/.local/bin/spacetime` → system PATH
2. **Port selection**: `TcpListener::bind("127.0.0.1:0")` for random available port
3. **Start**: `spacetime start --listen-addr 127.0.0.1:<port>`
4. **Health check**: Port probe — if `TcpListener::bind` fails, port is occupied = server listening
5. **Deploy**: `spacetime publish wasm-fantasia --project-path server --yes --delete-data -s http://addr`
6. **Shutdown**: `child.kill()` + `child.wait()`, also in `Drop` impl

Gated with `#[cfg(not(target_arch = "wasm32"))]`. Registered as plugin inside `NetworkingPlugin`.

**Files**: New `client/src/networking/local_server.rs`, `client/Cargo.toml` (added `home = "0.5"`)

---

## Phase 2: Singleplayer Connects to Local SpacetimeDB — CODE WRITTEN (blocked by Phase 1 crash)

**Goal**: Clicking "Singleplayer" on native starts the local server, deploys, connects, and enters gameplay.

### Title screen changes

**Native**: "Singleplayer" → `GameMode::Singleplayer` + `ServerTarget::Local` → starts local server → `Screen::Connecting`
**WASM**: "Solo" → `GameMode::Singleplayer` + `ServerTarget::Remote` → `Screen::Connecting`
**Both**: "Multiplayer" → `GameMode::Multiplayer` + `ServerTarget::Remote` → `Screen::Connecting`

### Connecting screen

Now handles both local server startup and remote connections:
- `advance_local_server` system drives `LocalServerState` (native only)
- `spawn_connecting_screen` reads `ServerTarget` to set URI and initial log message
- Systems chained: advance_local_server → track_connection_state → tick_connection → tick_timeout → update_log_display

### Networking run conditions generalized

- `OnEnter(Screen::Connecting)` reset timer: `resource_exists::<ServerTarget>` (was `is_multiplayer_mode`)
- `OnExit(Screen::Connecting)` disconnect: `resource_exists::<ServerTarget>` (was `is_multiplayer_mode`)
- `OnExit(Screen::Gameplay)` disconnect + remove target: `is_server_connected` (was `is_multiplayer_mode`)
- `auto_connect`: checks `ServerTarget` existence (was `GameMode::Multiplayer`); waits for `LocalServerState::Ready` on native

### Cleanup on exit

- `disconnect_from_spacetimedb` + `remove_server_target` on gameplay exit
- `shutdown_local_server` on gameplay exit (removes `LocalServer` + `LocalServerState`)
- `to::title` removes `ServerTarget`

**Files changed**: `client/src/screens/title.rs`, `client/src/screens/mod.rs`, `client/src/screens/connecting.rs`, `client/src/networking/mod.rs`

---

## Phase 3: Remove Client-Side Singleplayer Logic — CODE WRITTEN

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

## Phase 4: Web Solo Sessions (Server-Side) — NOT STARTED

**Goal**: Support "Solo" mode on web where the player gets a private session on the remote server.

### 4A: Server-side session isolation

`server/src/lib.rs`:
- Add a `session_id` field to `Player` table
- `join_game` reducer accepts an optional `session_id`
- Solo players get a unique session ID; multiplayer players share one
- Enemy spawning scoped to session: enemies belong to the session that spawned them
- `game_tick` AI only chases players in the same session
- Subscription queries filter by session

### 4B: Client sends session mode

`client/src/networking/mod.rs`:
- When calling `join_game`, pass `GameMode` so the server knows whether to create a private session or join the shared world

**Files**: `server/src/lib.rs`, `client/src/networking/mod.rs`

---

## Phase 5: Distribution & Polish — NOT STARTED

**Goal**: Native release builds are self-contained.

### 5A: Bundle SpacetimeDB binary

- macOS: Include in app bundle `Contents/MacOS/`
- Linux/Windows: Include alongside game executable
- `LocalServer` discovers binary relative to executable path first, then system PATH

### 5B: Pre-compile server module

Build step: `cargo build -p wasm_fantasia_module --target wasm32-unknown-unknown --release`
- Copy `.wasm` to `assets/server/wasm_fantasia_module.wasm`
- `LocalServer::deploy()` uses `spacetime publish --bin-path` instead of `--project-path`
- Eliminates Rust toolchain requirement on player machines

### 5C: Justfile targets

- `just bundle-native` — client + server module + SpacetimeDB binary
- `just bundle-web` — existing `web-build` (unchanged)

### 5D: Startup time optimization

- Start local server during asset loading (parallel)
- Keep server running across SP sessions (redeploy with `--delete-data` on new game)

**Files**: `Justfile`, `client/src/networking/local_server.rs`

---

## Dependency Graph

```
Phase 0 (Refactoring) ─── code written
    │
Phase 1 (Subprocess Manager) ─── code written, LOCAL SERVER CRASHES
    │
Phase 2 (SP Uses Local DB) ─── code written, blocked by Phase 1
    │
    ├── Phase 3 (Delete Client-Side SP Logic) ─── code written
    │
    ├── Phase 4 (Web Solo Sessions) ─── not started, server-side work
    │
    └── Phase 5 (Distribution) ─── not started, release packaging
```
