web_toolchain := "nightly-2026-05-01"

# Run native dev build
default: spacetimedb
    cargo run -p game-client


# Run WASM dev server
web: spacetimedb
    #!/usr/bin/env bash
    set -euo pipefail
    rustup toolchain install {{web_toolchain}} --profile minimal -c rust-src -t wasm32-unknown-unknown
    command -v bevy &>/dev/null || cargo install --git https://github.com/TheBevyFlock/bevy_cli --locked bevy_cli
    cd client && rustup run {{web_toolchain}} bevy run --yes --no-default-features --features web,dev web -U multi-threading --host 0.0.0.0 --open


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
    "{{spacetime}}" publish game-server \
        --module-path server \
        --yes \
        --delete-data=always

# Release build — native bundle in dist/native/, WASM bundle in dist/web/
build:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Building server WASM module..."
    cargo build -p game-server --target wasm32-unknown-unknown --release
    echo "Building native client..."
    cargo build -p game-client --release --no-default-features
    rm -rf dist/native && mkdir -p dist/native
    cp target/release/game-client dist/native/
    cp target/wasm32-unknown-unknown/release/game-server.wasm dist/native/
    cp "{{spacetime}}" dist/native/
    cp -r client/assets dist/native/
    echo "Native bundle ready at dist/native/"
    echo "Building WASM client..."
    rustup toolchain install {{web_toolchain}} --profile minimal -c rust-src -t wasm32-unknown-unknown
    command -v bevy &>/dev/null || cargo install --git https://github.com/TheBevyFlock/bevy_cli --locked bevy_cli
    cd client && rustup run {{web_toolchain}} bevy build --yes --no-default-features --features web --release web -U multi-threading --bundle
    echo "WASM bundle ready at dist/web/"

# Per-system profiling — press F9 in-game for timing breakdown
profile: spacetimedb
    cargo run -p game-client --features profile

# Pre-commit checks: lint + web compilation
check: verify
    cargo clippy --workspace -- -D warnings
    cargo fmt --all -- --check
    cargo machete
    cargo check -p game-client --profile ci --no-default-features --features web --target wasm32-unknown-unknown

# Verify the gameplay laws and rejection cases
verify:
    uv run --no-project python core/verification/check.py --verus "${VERUS:-verus}"

# Analyze web build sizes
web-size *args:
    python3 client/web_size.py {{args}}

# Regenerate SpacetimeDB client bindings
generate:
    #!/usr/bin/env bash
    set -euo pipefail
    "{{spacetime}}" generate --lang rust --module-path server --out-dir client/networking/src/generated --yes
    echo "Bindings regenerated."


# Deploy to production
deploy: build-web
    #!/usr/bin/env bash
    set -euo pipefail
    just deploy-client
    # Server: docker cp into container, then publish
    if [[ "$(hostname)" == thinkcentre ]]; then
        docker cp target/wasm32-unknown-unknown/release/game_server.wasm spacetimedb:/tmp/game-server.wasm
        docker exec spacetimedb spacetime publish --server http://localhost:3000 --bin-path /tmp/game-server.wasm --yes game-server
    else
        scp -q target/wasm32-unknown-unknown/release/game_server.wasm thinkcentre:/tmp/game-server.wasm
        ssh thinkcentre "docker cp /tmp/game-server.wasm spacetimedb:/tmp/game-server.wasm && docker exec spacetimedb spacetime publish --server http://localhost:3000 --bin-path /tmp/game-server.wasm --yes game-server"
    fi

# Publish a built client without rebuilding the server
deploy-client:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "$(hostname)" == thinkcentre ]]; then
        game_source="$PWD/target/bevy_web/web-release/game-client"
        game_publish=(bash)
    else
        rsync -az --delete target/bevy_web/web-release/game-client/ thinkcentre:/srv/game/
        game_source=/srv/game
        game_publish=(ssh thinkcentre bash)
    fi
    "${game_publish[@]}" -s -- "$game_source" <<'REMOTE'
    set -euo pipefail
    game_source="$1"
    game_volume="$(docker volume inspect caddy_game_web | jq -r '.[0].Mountpoint')"
    game_release="$(sha256sum "$game_source/build/game-client_bg.wasm" | cut -c1-12)"
    sudo mkdir -p "$game_volume/build/$game_release"
    sudo rsync -a "$game_source/build/" "$game_volume/build/$game_release/"
    sudo rsync -a "$game_source/assets/" "$game_volume/assets/"
    sed "s|./build/|./build/$game_release/|g" "$game_source/index.html" | sudo tee "$game_volume/index.html.next" >/dev/null
    sudo mv "$game_volume/index.html.next" "$game_volume/index.html"
    REMOTE

# Build WASM client + server module (server without thread flags for SpacetimeDB)
build-web:
    #!/usr/bin/env bash
    set -euo pipefail
    CARGO_ENCODED_RUSTFLAGS="" cargo +stable build -p game-server --target wasm32-unknown-unknown --release
    rustup toolchain install {{web_toolchain}} --profile minimal -c rust-src -t wasm32-unknown-unknown
    cd client && rustup run {{web_toolchain}} bevy build --yes --no-default-features --features web --release web -U multi-threading --bundle

# Wipe SpacetimeDB data and redeploy module
db-reset:
    "{{spacetime}}" publish game-server --module-path server --yes --delete-data=always
