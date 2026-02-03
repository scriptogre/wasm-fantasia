# Run native dev build
default:
    cargo run --features dev_native

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

    # Deploy module
    "{{spacetime}}" publish wasm-fantasia \
        --project-path server \
        --yes \
        --delete-data=on-conflict

    # Launch two game clients
    cargo run --features dev_native &
    cargo run --features dev_native &

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
    cargo build --release

# Pre-commit checks: lint + web compilation
check:
    cargo clippy -- -D warnings
    cargo fmt --all -- --check
    cargo machete
    cargo check --profile ci --no-default-features --features web --target wasm32-unknown-unknown
