# Run native dev build
default: spacetimedb
    cargo run -p wasm_fantasia --features dev_native


# Run WASM dev server
web: spacetimedb
    #!/usr/bin/env bash
    set -euo pipefail
    rustup toolchain install nightly --profile minimal -c rust-src 2>/dev/null || true
    command -v bevy &>/dev/null || cargo install --git https://github.com/TheBevyFlock/bevy_cli --locked bevy_cli
    cd client && rustup run nightly bevy run --yes --no-default-features --features web web -U multi-threading --host 0.0.0.0 --open


spacetime := env('HOME') / ".local/bin/spacetime"

# Ensure SpacetimeDB is running and module is deployed
spacetimedb:
    #!/usr/bin/env bash
    set -euo pipefail
    command -v "{{spacetime}}" &>/dev/null || \
        (echo "Installing SpacetimeDB..." && curl -sSf https://install.spacetimedb.com | sh)
    if ! "{{spacetime}}" start --pg-port 5432 2>/dev/null & then true; fi
    sleep 2
    "{{spacetime}}" publish wasm-fantasia \
        --project-path server \
        --yes \
        --delete-data
    TOKEN=$(grep spacetimedb_token ~/.config/spacetime/cli.toml | cut -d'"' -f2)
    echo ""
    echo "═══════════════════════════════════════════════════════════════════"
    echo "  Postgres: postgresql://token:${TOKEN}@localhost:5432/wasm-fantasia"
    echo "═══════════════════════════════════════════════════════════════════"
    echo ""

# Native release build
build:
    cargo build -p wasm_fantasia --release

# WASM release build
web-build:
    #!/usr/bin/env bash
    set -euo pipefail
    rustup toolchain install nightly --profile minimal -c rust-src 2>/dev/null || true
    command -v bevy &>/dev/null || cargo install --git https://github.com/TheBevyFlock/bevy_cli --locked bevy_cli
    cd client && rustup run nightly bevy build --yes --no-default-features --features web --release web -U multi-threading --bundle

# Pre-commit checks: lint + web compilation
check:
    cargo clippy --workspace -- -D warnings
    cargo fmt --all -- --check
    cargo machete
    cargo check -p wasm_fantasia --profile ci --no-default-features --features web --target wasm32-unknown-unknown

# Analyze web build sizes
web-size *args:
    python3 client/web_size.py {{args}}

# Wipe SpacetimeDB data and redeploy module
db-reset:
    "{{spacetime}}" publish wasm-fantasia --project-path server --yes --delete-data
