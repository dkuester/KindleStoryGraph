# KindleStoryGraph build & package tooling.

TARGET := "armv7-unknown-linux-musleabihf"
EXT := "packaging/extension"

# Cross-compile the static armhf (musl) binary via Docker.
# Produces target-armv7/<TARGET>/release/kindlestorygraph.
build:
    docker run --rm -v "$PWD":/app -w /app -v ksg-cargo-registry:/root/.cargo/registry messense/rust-musl-cross:armv7-musleabihf cargo build --release --target {{TARGET}} --target-dir /app/target-armv7

# Assemble the KUAL extension folder (requires `just build` first).
package:
    #!/usr/bin/env sh
    set -e
    mkdir -p "{{EXT}}/bin"
    cp "target-armv7/{{TARGET}}/release/kindlestorygraph" "{{EXT}}/bin/kindlestorygraph"
    cp config_example.ini "{{EXT}}/config_example.ini"
    cp manual_mappings.example.ini "{{EXT}}/manual_mappings.example.ini"
    chmod +x "{{EXT}}/bin/kindlestorygraph"
    echo "Extension assembled in {{EXT}}/ — copy it to /mnt/us/extensions/kindlestorygraph"

# Host-side static checks: format + clippy (warnings as errors) + unit tests.
lint:
    cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
