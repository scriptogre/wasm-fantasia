# Run native dev build
default:
    cargo run -p wasm_fantasia --features dev_native

# Multiplayer: start server, publish module, launch two clients
spacetime := env('HOME') / ".local/bin/spacetime"
mp:
    #!/usr/bin/env bash
    set -euo pipefail

    # Ensure SpacetimeDB is installed
    command -v "{{spacetime}}" &>/dev/null || \
        (echo "Installing SpacetimeDB..." && curl -sSf https://install.spacetimedb.com | sh)

    # Start server
    "{{spacetime}}" start --pg-port 5432 &
    sleep 2

    # Deploy module (always wipe data for clean start)
    "{{spacetime}}" publish wasm-fantasia \
        --project-path server \
        --yes \
        --delete-data

    # Launch two game clients
    cargo run -p wasm_fantasia --features dev_native &
    cargo run -p wasm_fantasia --features dev_native &

    # Print Postgres connection string (after clients start so it's visible)
    TOKEN=$(grep spacetimedb_token ~/.config/spacetime/cli.toml | cut -d'"' -f2)
    echo ""
    echo "═══════════════════════════════════════════════════════════════════"
    echo "  Postgres: postgresql://token:${TOKEN}@localhost:5432/wasm-fantasia"
    echo "═══════════════════════════════════════════════════════════════════"
    echo ""

    wait

# Native release build
build:
    cargo build -p wasm_fantasia --release

# Pre-commit checks: lint + web compilation + web model verification
check:
    cargo clippy --workspace -- -D warnings
    cargo fmt --all -- --check
    cargo machete
    cargo check -p wasm_fantasia --profile ci --no-default-features --features web --target wasm32-unknown-unknown
    node scripts/build-web-model.mjs --verify

# Regenerate player-web.glb from player.glb (strips unused animations, quantizes)
web-model:
    node scripts/build-web-model.mjs

# Run WASM dev server
web:
    #!/usr/bin/env bash
    set -euo pipefail
    rustup toolchain install nightly --profile minimal -c rust-src 2>/dev/null || true
    command -v bevy &>/dev/null || cargo install --git https://github.com/TheBevyFlock/bevy_cli --locked bevy_cli
    cd client && rustup run nightly bevy run --yes --no-default-features --features web web -U multi-threading --host 0.0.0.0 --open

# WASM multiplayer: start server, deploy module, launch WASM dev server
web-mp:
    #!/usr/bin/env bash
    set -euo pipefail

    # Ensure SpacetimeDB is installed
    command -v "{{spacetime}}" &>/dev/null || \
        (echo "Installing SpacetimeDB..." && curl -sSf https://install.spacetimedb.com | sh)

    # Start server
    "{{spacetime}}" start --pg-port 5432 &
    sleep 2

    # Deploy module (always wipe data for clean start)
    "{{spacetime}}" publish wasm-fantasia \
        --project-path server \
        --yes \
        --delete-data

    # Ensure WASM toolchain and bevy CLI
    rustup toolchain install nightly --profile minimal -c rust-src 2>/dev/null || true
    command -v bevy &>/dev/null || cargo install --git https://github.com/TheBevyFlock/bevy_cli --locked bevy_cli

    # Launch WASM dev server (open browser, then open a second tab manually)
    cd client && rustup run nightly bevy run --yes --no-default-features --features web web -U multi-threading --host 0.0.0.0 --open

# Build WASM release
web-build:
    #!/usr/bin/env bash
    set -euo pipefail
    rustup toolchain install nightly --profile minimal -c rust-src 2>/dev/null || true
    command -v bevy &>/dev/null || cargo install --git https://github.com/TheBevyFlock/bevy_cli --locked bevy_cli
    cd client && rustup run nightly bevy build --yes --no-default-features --features web --release web -U multi-threading --bundle

# Wipe SpacetimeDB data and redeploy module
db-reset:
    "{{spacetime}}" publish wasm-fantasia --project-path server --yes --delete-data
