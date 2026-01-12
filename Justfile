# Default recipe - run dev native build
default:
    cargo run

# Clean rebuild + lint + web check
clean-build:
    cargo clean
    just build-dev
    just lint
    just check-web

# Generate and open documentation
docs:
    cargo doc --open --no-deps --workspace

# Run linters: clippy, fmt check, machete (unused deps)
lint:
    cargo clippy -- -D warnings
    cargo fmt --all -- --check
    cargo machete

# Debug build
build-dev:
    cargo build

# Release build
build:
    cargo build --release

# Check web compilation (fast, doesn't build wasm)
check-web:
    cargo check --profile ci --no-default-features --features web --target wasm32-unknown-unknown

# Full web release build with wasm tools
build-web:
    cargo binstall --locked -y --force wasm-bindgen-cli
    cargo binstall --locked -y --force wasm-opt
    bevy build --locked --release --features=web --yes web --bundle

# Run with hot patching (dx serve)
hot:
    BEVY_ASSET_ROOT="." dx serve --hot-patch

# Run dev native build
run:
    cargo run

# Run web with trunk (current working method)
run-web:
    trunk serve --port 8080

# Run web with SharedArrayBuffer headers (original bevy method)
run-web-headers:
    bevy run web --headers="Cross-Origin-Opener-Policy:same-origin" --headers="Cross-Origin-Embedder-Policy:credentialless"
