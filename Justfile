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
    # Start server if port 3000 isn't already listening
    if nc -z 127.0.0.1 3000 2>/dev/null; then
        echo "SpacetimeDB already running on port 3000"
    else
        "{{spacetime}}" start 2>/dev/null &
        echo "Waiting for SpacetimeDB..."
        for i in $(seq 1 30); do
            if nc -z 127.0.0.1 3000 2>/dev/null; then break; fi
            sleep 0.5
        done
        if ! nc -z 127.0.0.1 3000 2>/dev/null; then
            echo "ERROR: SpacetimeDB failed to start on port 3000"
            exit 1
        fi
    fi
    "{{spacetime}}" publish wasm-fantasia \
        --project-path server \
        --yes \
        --delete-data

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

# Regenerate SpacetimeDB client bindings (patches WASM-incompatible methods)
generate:
    #!/usr/bin/env bash
    set -euo pipefail
    "{{spacetime}}" generate --lang rust --project-path server --out-dir client/src/networking/generated --yes
    # The codegen emits advance_one_message_blocking() and run_threaded() which
    # don't exist in our WASM-patched SDK fork. Gate them to native-only.
    sed -i '' 's/    pub fn advance_one_message_blocking/    #[cfg(not(target_arch = "wasm32"))]\n    pub fn advance_one_message_blocking/' client/src/networking/generated/mod.rs
    sed -i '' 's/    pub fn run_threaded/    #[cfg(not(target_arch = "wasm32"))]\n    pub fn run_threaded/' client/src/networking/generated/mod.rs
    echo "Bindings regenerated and WASM-patched."

# Wipe SpacetimeDB data and redeploy module
db-reset:
    "{{spacetime}}" publish wasm-fantasia --project-path server --yes --delete-data
